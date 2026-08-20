use anyhow::Context;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use std::io::Cursor;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use url::Url;

use crate::api::Track;
use crate::auth::{Token, try_refresh_token};
use crate::player::stream::cache::{CachedHls, SEGMENT_CACHE_CAP, SegmentCache};
use crate::player::stream::downloader::spawn_segment_pump;
use crate::player::stream::hls::{HlsManifest, StreamsResponse};
use crate::player::stream::sample::TapSource;

pub(crate) const CROSSFADE_DURATION: Duration = Duration::from_millis(35);
const CROSSFADE_STEPS: usize = 7;

fn resolve_transcoding_url(
    client: &reqwest::blocking::Client,
    resolver_url: &str,
    access_token: &str,
) -> anyhow::Result<Url> {
    let response: serde_json::Value = client
        .get(resolver_url)
        .header(
            reqwest::header::AUTHORIZATION,
            crate::auth::authorization_header(access_token),
        )
        .send()
        .context("failed to resolve SoundCloud transcoding")?
        .error_for_status()
        .context("SoundCloud transcoding resolver returned error status")?
        .json()
        .context("failed to parse SoundCloud transcoding response")?;
    let media_url = response
        .get("url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .context("SoundCloud transcoding response contained no media URL")?;
    Url::parse(media_url).context("invalid SoundCloud media URL")
}

pub(crate) fn open_output_stream() -> Arc<Mutex<OutputStream>> {
    let output_stream = OutputStreamBuilder::open_default_stream().unwrap();
    Arc::new(Mutex::new(output_stream))
}

pub(crate) struct PlaybackEngine {
    stream: Arc<Mutex<OutputStream>>,
    client: reqwest::blocking::Client,
    generation: Arc<AtomicU64>,
    cache: Option<CachedHls>,
    preload_next: Option<CachedHls>,
}

impl PlaybackEngine {
    pub(crate) fn new(stream: Arc<Mutex<OutputStream>>) -> anyhow::Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("sctui")
            .timeout(Duration::from_secs(15))
            .build()
            .context("failed to build reqwest client")?;

        Ok(Self {
            stream,
            client,
            generation: Arc::new(AtomicU64::new(0)),
            cache: None,
            preload_next: None,
        })
    }

    fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn get_hls_url(&self, track: &Track, access_token: &str) -> anyhow::Result<Url> {
        if !track.stream_url.is_empty() {
            return resolve_transcoding_url(&self.client, &track.stream_url, access_token);
        }

        let track_urn = &track.track_urn;
        let streams_url = format!("https://api.soundcloud.com/tracks/{}/streams", track_urn);
        let streams_response: StreamsResponse = self
            .client
            .get(&streams_url)
            .header(
                reqwest::header::AUTHORIZATION,
                crate::auth::authorization_header(access_token),
            )
            .send()
            .context("failed to fetch streams endpoint")?
            .error_for_status()
            .context("streams endpoint returned error status")?
            .json()
            .context("failed to parse streams response json")?;

        let hls_url = streams_response
            .hls_aac_160_url
            .or(streams_response.hls_aac_96_url)
            .or(streams_response.hls_mp3_128_url)
            .ok_or_else(|| {
                anyhow::anyhow!("No HLS stream URL available (tried AAC 160, AAC 96, MP3 128)")
            })?;

        Url::parse(&hls_url).context("invalid HLS URL")
    }

    fn download_bytes(&self, url: &Url) -> anyhow::Result<Vec<u8>> {
        let bytes = self
            .client
            .get(url.as_str())
            .send()
            .with_context(|| format!("failed to download {}", url))?
            .error_for_status()
            .with_context(|| format!("download returned error status {}", url))?
            .bytes()
            .with_context(|| format!("failed to read bytes {}", url))?;
        Ok(bytes.to_vec())
    }

    fn ensure_cached_hls(
        &mut self,
        track: &Track,
        token: &Arc<Mutex<Token>>,
    ) -> anyhow::Result<(Arc<HlsManifest>, Arc<Vec<u8>>, Arc<Mutex<SegmentCache>>)> {
        let now = Instant::now();

        if let Some(ref preload) = self.preload_next {
            if preload.track_urn == track.track_urn {
                let preload = self.preload_next.take().unwrap();
                self.cache = Some(preload);
                let cached = self.cache.as_ref().unwrap();
                return Ok((
                    Arc::clone(&cached.manifest),
                    Arc::clone(&cached.init_bytes),
                    Arc::clone(&cached.segment_cache),
                ));
            }
        }

        let cache_valid = self
            .cache
            .as_ref()
            .is_some_and(|c| c.is_valid_for(track, now));

        if !cache_valid {
            let _ = try_refresh_token(token);
            let access_token = { token.lock().unwrap().access_token.clone() };

            let playlist_url = self.get_hls_url(track, &access_token)?;
            let manifest = HlsManifest::fetch(&self.client, &playlist_url, &access_token)?;

            let init_bytes = if let Some(init_url) = &manifest.init_url {
                Arc::new(self.download_bytes(init_url)?)
            } else {
                Arc::new(Vec::new())
            };

            self.cache = Some(CachedHls {
                track_urn: track.track_urn.clone(),
                fetched_at: now,
                manifest: Arc::new(manifest),
                init_bytes,
                segment_cache: Arc::new(Mutex::new(SegmentCache::new(SEGMENT_CACHE_CAP))),
            });
        }

        let cached = self.cache.as_ref().expect("cache must be set by now");
        Ok((
            Arc::clone(&cached.manifest),
            Arc::clone(&cached.init_bytes),
            Arc::clone(&cached.segment_cache),
        ))
    }

    pub(crate) fn preload_next_track(
        &mut self,
        track: &Track,
        token: &Arc<Mutex<Token>>,
    ) -> anyhow::Result<()> {
        if self
            .preload_next
            .as_ref()
            .is_some_and(|p| p.track_urn == track.track_urn)
        {
            return Ok(());
        }

        let _ = try_refresh_token(token);
        let access_token = { token.lock().unwrap().access_token.clone() };

        let playlist_url = self.get_hls_url(track, &access_token)?;
        let manifest = HlsManifest::fetch(&self.client, &playlist_url, &access_token)?;

        let init_bytes = if let Some(init_url) = &manifest.init_url {
            Arc::new(self.download_bytes(init_url)?)
        } else {
            Arc::new(Vec::new())
        };

        let first_segment_bytes = if !manifest.segments.is_empty() {
            match self.download_bytes(&manifest.segments[0].url) {
                Ok(bytes) => {
                    let mut cache = SegmentCache::new(SEGMENT_CACHE_CAP);
                    cache.insert(0, Arc::new(bytes));
                    Some(Arc::new(Mutex::new(cache)))
                }
                Err(_) => None,
            }
        } else {
            None
        };

        self.preload_next = Some(CachedHls {
            track_urn: track.track_urn.clone(),
            fetched_at: Instant::now(),
            manifest: Arc::new(manifest),
            init_bytes,
            segment_cache: first_segment_bytes
                .unwrap_or_else(|| Arc::new(Mutex::new(SegmentCache::new(SEGMENT_CACHE_CAP)))),
        });

        Ok(())
    }

    pub(crate) fn play_from_position(
        &mut self,
        track: &Track,
        position_ms: u64,
        token: &Arc<Mutex<Token>>,
        sink_arc: &Arc<Mutex<Option<Sink>>>,
        is_playing_flag: &Arc<std::sync::atomic::AtomicBool>,
        elapsed_time: &Arc<Mutex<Duration>>,
        last_start: &Arc<Mutex<Option<Instant>>>,
        current_track: &Arc<Mutex<Option<Track>>>,
        wave_buffer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
    ) -> anyhow::Result<()> {
        let old_track_urn = current_track
            .lock()
            .unwrap()
            .as_ref()
            .map(|t| t.track_urn.clone());
        let is_seek = old_track_urn.as_deref() == Some(&track.track_urn);

        let (has_old_sink, target_volume) = {
            let guard = sink_arc.lock().unwrap();
            let vol = guard.as_ref().map(|s| s.volume()).unwrap_or(1.0);
            (guard.is_some(), vol)
        };

        let planned_generation = self.current_generation().wrapping_add(1);
        if !is_seek {
            let generation_id = self.bump_generation();
            debug_assert_eq!(generation_id, planned_generation);
            if let Some(ref s) = *sink_arc.lock().unwrap() {
                s.stop();
            }
        }

        let (manifest, init_bytes, segment_cache) =
            self.ensure_cached_hls(track, token).map_err(|error| {
                if !is_seek {
                    is_playing_flag.store(false, Ordering::SeqCst);
                    *last_start.lock().unwrap() = None;
                }
                error.context("failed to prepare track for playback")
            })?;

        let (segment_index, offset_within_segment_ms) = manifest.locate_position(position_ms);

        let media_bytes = {
            let mut cache_guard = segment_cache.lock().unwrap();
            if let Some(bytes) = cache_guard.get(segment_index) {
                bytes
            } else {
                drop(cache_guard);
                let bytes = self
                    .download_bytes(&manifest.segments[segment_index].url)
                    .map_err(|error| {
                        if !is_seek {
                            is_playing_flag.store(false, Ordering::SeqCst);
                            *last_start.lock().unwrap() = None;
                        }
                        error.context("failed to download first media segment")
                    })?;
                let arc = Arc::new(bytes);
                let mut cache_guard = segment_cache.lock().unwrap();
                cache_guard.insert(segment_index, Arc::clone(&arc));
                arc
            }
        };

        let first_bytes = combine_init_and_segment(&init_bytes, &media_bytes);

        let new_sink = {
            let stream_guard = self.stream.lock().unwrap();
            Sink::connect_new(stream_guard.mixer())
        };

        new_sink.set_volume(target_volume);

        append_segment_to_sink(
            &new_sink,
            first_bytes,
            wave_buffer,
            offset_within_segment_ms,
        )
        .map_err(|error| {
            if !is_seek {
                is_playing_flag.store(false, Ordering::SeqCst);
                *last_start.lock().unwrap() = None;
            }
            error.context("failed to decode first media segment")
        })?;

        let gen_for_pump = if is_seek {
            let generation_id = self.bump_generation();
            debug_assert_eq!(generation_id, planned_generation);
            generation_id
        } else {
            planned_generation
        };

        let old_sink_for_fade = if is_seek && has_old_sink {
            sink_arc.lock().unwrap().take()
        } else {
            None
        };

        *sink_arc.lock().unwrap() = Some(new_sink);

        if let Some(old_sink) = old_sink_for_fade {
            crossfade_and_stop(old_sink, target_volume);
        }

        *current_track.lock().unwrap() = Some(track.clone());
        *elapsed_time.lock().unwrap() = Duration::from_millis(position_ms);
        *last_start.lock().unwrap() = Some(Instant::now());
        is_playing_flag.store(true, Ordering::SeqCst);

        use crate::player::stream::downloader::SegmentPumpParams;
        spawn_segment_pump(SegmentPumpParams {
            client: self.client.clone(),
            generation: Arc::clone(&self.generation),
            generation_value: gen_for_pump,
            manifest,
            init_bytes,
            segment_cache,
            start_segment_index: segment_index,
            sink_arc: Arc::clone(sink_arc),
            wave_buffer: Arc::clone(wave_buffer),
            is_playing_flag: Arc::clone(is_playing_flag),
            elapsed_time: Arc::clone(elapsed_time),
            last_start: Arc::clone(last_start),
        });
        Ok(())
    }
}

fn combine_init_and_segment(init_bytes: &[u8], segment_bytes: &[u8]) -> Vec<u8> {
    let mut combined = Vec::with_capacity(init_bytes.len() + segment_bytes.len());
    combined.extend_from_slice(init_bytes);
    combined.extend_from_slice(segment_bytes);
    combined
}

fn append_segment_to_sink(
    sink: &Sink,
    bytes: Vec<u8>,
    wave_buffer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
    skip_ms: u64,
) -> anyhow::Result<()> {
    let cursor = Cursor::new(bytes);
    let decoder = Decoder::new(cursor).context("rodio decoder init failed")?;

    if skip_ms > 0 {
        let skipped = decoder.skip_duration(Duration::from_millis(skip_ms));
        let tapped = TapSource::new(skipped, Arc::clone(wave_buffer));
        sink.append(tapped);
    } else {
        let tapped = TapSource::new(decoder, Arc::clone(wave_buffer));
        sink.append(tapped);
    }

    Ok(())
}

fn crossfade_and_stop(old_sink: Sink, target_volume: f32) {
    let steps = CROSSFADE_STEPS.max(1);
    let total_ms = CROSSFADE_DURATION.as_millis().max(1) as u64;
    let step_ms = (total_ms / steps as u64).max(1);

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        old_sink.set_volume(target_volume * (1.0 - t));
        std::thread::sleep(Duration::from_millis(step_ms));
    }

    old_sink.stop();
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use super::{Decoder, HlsManifest, combine_init_and_segment, resolve_transcoding_url};
    use std::io::Cursor;

    #[test]
    fn transcoding_resolver_uses_browser_oauth_and_returns_signed_hls_url() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let resolver_url = format!("http://{}/resolver", server.server_addr().to_ip().unwrap());
        let observed_auth = Arc::new(Mutex::new(None));
        let server_auth = Arc::clone(&observed_auth);
        let responder = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .expect("resolver request");
            *server_auth.lock().unwrap() = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string());
            request
                .respond(
                    Response::from_string(r#"{"url":"https://cdn.example/signed.m3u8"}"#)
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json").unwrap(),
                        ),
                )
                .unwrap();
        });

        let client = reqwest::blocking::Client::new();
        let resolved = resolve_transcoding_url(&client, &resolver_url, "browser-token").unwrap();
        responder.join().unwrap();

        assert_eq!(resolved.as_str(), "https://cdn.example/signed.m3u8");
        assert_eq!(
            *observed_auth.lock().unwrap(),
            Some("OAuth browser-token".into())
        );
    }

    #[test]
    #[ignore = "uses Peter's live Firefox session and SoundCloud API"]
    fn live_browser_session_prepares_a_decodable_liked_track_segment() {
        let token = Arc::new(Mutex::new(crate::auth::initial_token().unwrap()));
        let access_token = token.lock().unwrap().access_token.clone();
        let client = reqwest::blocking::Client::new();
        let tracks = crate::api::API::init(token).get_liked_tracks().unwrap();

        let found_decodable = tracks
            .into_iter()
            .filter(|track| !track.stream_url.is_empty())
            .take(20)
            .any(|track| {
                let decoded_samples = (|| -> anyhow::Result<usize> {
                    let resolved =
                        resolve_transcoding_url(&client, &track.stream_url, &access_token)?;
                    let manifest = HlsManifest::fetch(&client, &resolved, &access_token)?;
                    let init_bytes = manifest
                        .init_url
                        .as_ref()
                        .map(|url| {
                            client
                                .get(url.as_str())
                                .send()?
                                .error_for_status()?
                                .bytes()
                                .map(|bytes| bytes.to_vec())
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let segment = manifest
                        .segments
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("HLS manifest contained no segments"))?;
                    let segment_bytes = client
                        .get(segment.url.as_str())
                        .send()?
                        .error_for_status()?
                        .bytes()?;
                    let bytes = combine_init_and_segment(&init_bytes, &segment_bytes);
                    Ok(Decoder::new(Cursor::new(bytes))?.take(100).count())
                })();

                matches!(decoded_samples, Ok(100))
            });

        assert!(
            found_decodable,
            "first twenty supported liked tracks had no decodable HLS segment"
        );
    }

    #[test]
    #[ignore = "uses Peter's live Firefox session and SoundCloud library"]
    fn live_go_plus_track_is_classified_before_its_dead_legacy_resolver() {
        let token = Arc::new(Mutex::new(crate::auth::initial_token().unwrap()));
        let mut api = crate::api::API::init(Arc::clone(&token));
        let track = api
            .get_liked_tracks()
            .unwrap()
            .into_iter()
            .find(|track| {
                track.playback_restriction
                    == Some(crate::api::PlaybackRestriction::SoundCloudGoPlus)
            })
            .expect("current liked-track page contains a Go+ track");

        assert_eq!(
            track.playback_restriction,
            Some(crate::api::PlaybackRestriction::SoundCloudGoPlus)
        );
        assert!(!track.is_playable());
    }
}
