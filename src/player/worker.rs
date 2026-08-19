use crate::api::Track;
use crate::auth::Token;
use rodio::Sink;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};
use std::time::{Duration, Instant};

use super::commands::PlayerCommand;
use super::stream::{PlaybackEngine, open_output_stream};
use super::PlaybackOutcome;

pub(crate) fn record_playback_result(
    last_error: &Arc<Mutex<Option<String>>>,
    outcomes: &Arc<Mutex<VecDeque<PlaybackOutcome>>>,
    attempt_id: u64,
    track_urn: &str,
    result: anyhow::Result<()>,
) {
    let outcome = match result {
        Ok(()) => {
            *last_error.lock().unwrap() = None;
            PlaybackOutcome::Started {
                attempt_id,
                track_urn: track_urn.to_string(),
            }
        }
        Err(error) => {
            let message = format!("{error:#}");
            *last_error.lock().unwrap() = Some(message.clone());
            PlaybackOutcome::Failed {
                attempt_id,
                track_urn: track_urn.to_string(),
                message,
            }
        }
    };
    outcomes.lock().unwrap().push_back(outcome);
}

pub(crate) fn player_loop(
    rx: Receiver<PlayerCommand>,
    token: Arc<Mutex<Token>>,
    is_playing_flag: Arc<AtomicBool>,
    is_seeking_flag: Arc<AtomicBool>,
    sink_arc: Arc<Mutex<Option<Sink>>>,
    elapsed_time: Arc<Mutex<Duration>>,
    last_start: Arc<Mutex<Option<Instant>>>,
    current_track: Arc<Mutex<Option<Track>>>,
    wave_buffer: Arc<Mutex<VecDeque<f32>>>,
    last_error: Arc<Mutex<Option<String>>>,
    outcomes: Arc<Mutex<VecDeque<PlaybackOutcome>>>,
) {
    let stream = open_output_stream();
    let mut engine = PlaybackEngine::new(Arc::clone(&stream)).unwrap();
    let mut active_attempt_id = 0;

    for msg in rx {
        match msg {
            PlayerCommand::Play(track, attempt_id) => {
                active_attempt_id = attempt_id;
                let result = engine.play_from_position(
                    &track,
                    0,
                    &token,
                    &sink_arc,
                    &is_playing_flag,
                    &elapsed_time,
                    &last_start,
                    &current_track,
                    &wave_buffer,
                );
                record_playback_result(
                    &last_error,
                    &outcomes,
                    attempt_id,
                    &track.track_urn,
                    result,
                );
            }

            PlayerCommand::PlayFromPosition(track, position_ms, attempt_id) => {
                if is_seeking_flag.swap(true, Ordering::SeqCst) {
                    continue;
                }
                active_attempt_id = attempt_id;
                let result = engine.play_from_position(
                    &track,
                    position_ms,
                    &token,
                    &sink_arc,
                    &is_playing_flag,
                    &elapsed_time,
                    &last_start,
                    &current_track,
                    &wave_buffer,
                );
                record_playback_result(
                    &last_error,
                    &outcomes,
                    attempt_id,
                    &track.track_urn,
                    result,
                );
                is_seeking_flag.store(false, Ordering::SeqCst);
            }

            PlayerCommand::PreloadNext(track) => {
                let _ = engine.preload_next_track(&track, &token);
            }

            PlayerCommand::Pause => {
                if let Some(ref s) = *sink_arc.lock().unwrap() {
                    s.pause();
                    is_playing_flag.store(false, Ordering::SeqCst);

                    if let Some(start) = *last_start.lock().unwrap() {
                        let mut elapsed = elapsed_time.lock().unwrap();
                        *elapsed += start.elapsed();
                    }
                    *last_start.lock().unwrap() = None;
                }
            }

            PlayerCommand::Resume => {
                if let Some(ref s) = *sink_arc.lock().unwrap() {
                    s.play();
                    is_playing_flag.store(true, Ordering::SeqCst);
                    *last_start.lock().unwrap() = Some(Instant::now());
                }
            }

            PlayerCommand::VolumeUp => {
                if let Some(ref s) = *sink_arc.lock().unwrap() {
                    let new_volume = (s.volume() + 0.1).min(2.0);
                    s.set_volume(new_volume);
                }
            }

            PlayerCommand::VolumeDown => {
                if let Some(ref s) = *sink_arc.lock().unwrap() {
                    let new_volume = (s.volume() - 0.1).max(0.0);
                    s.set_volume(new_volume);
                }
            }

            PlayerCommand::NextSong => {}

            PlayerCommand::PrevSong => {}

            PlayerCommand::FastForward => {
                if is_seeking_flag.swap(true, Ordering::SeqCst) {
                    continue;
                }
                let current_track_guard = current_track.lock().unwrap();
                if let Some(track) = current_track_guard.clone() {
                    drop(current_track_guard);

                    let elapsed = elapsed_time.lock().unwrap();
                    let current_elapsed = if is_playing_flag.load(Ordering::SeqCst) {
                        if let Some(start) = *last_start.lock().unwrap() {
                            *elapsed + start.elapsed()
                        } else {
                            *elapsed
                        }
                    } else {
                        *elapsed
                    };

                    let new_elapsed = current_elapsed + Duration::from_secs(10);
                    let max_duration = Duration::from_millis(track.duration_ms);

                    drop(elapsed);

                    if new_elapsed >= max_duration {
                        if let Some(ref s) = *sink_arc.lock().unwrap() {
                            s.stop();
                        }
                        is_playing_flag.store(false, Ordering::SeqCst);
                        *elapsed_time.lock().unwrap() = max_duration;
                        *last_start.lock().unwrap() = None;
                    } else {
                        let new_position_ms = new_elapsed.as_millis() as u64;
                        let result = engine.play_from_position(
                            &track,
                            new_position_ms,
                            &token,
                            &sink_arc,
                            &is_playing_flag,
                            &elapsed_time,
                            &last_start,
                            &current_track,
                            &wave_buffer,
                        );
                        record_playback_result(
                            &last_error,
                            &outcomes,
                            active_attempt_id,
                            &track.track_urn,
                            result,
                        );
                    }
                }
                is_seeking_flag.store(false, Ordering::SeqCst);
            }

            PlayerCommand::Rewind => {
                if is_seeking_flag.swap(true, Ordering::SeqCst) {
                    continue;
                }
                let current_track_guard = current_track.lock().unwrap();
                if let Some(track) = current_track_guard.clone() {
                    drop(current_track_guard);

                    let elapsed = elapsed_time.lock().unwrap();
                    let current_elapsed = if is_playing_flag.load(Ordering::SeqCst) {
                        if let Some(start) = *last_start.lock().unwrap() {
                            *elapsed + start.elapsed()
                        } else {
                            *elapsed
                        }
                    } else {
                        *elapsed
                    };

                    let rewind_duration = Duration::from_secs(10);
                    let new_elapsed = if current_elapsed > rewind_duration {
                        current_elapsed - rewind_duration
                    } else {
                        Duration::ZERO
                    };

                    drop(elapsed);

                    let new_position_ms = new_elapsed.as_millis() as u64;
                    let result = engine.play_from_position(
                        &track,
                        new_position_ms,
                        &token,
                        &sink_arc,
                        &is_playing_flag,
                        &elapsed_time,
                        &last_start,
                        &current_track,
                        &wave_buffer,
                    );
                    record_playback_result(
                        &last_error,
                        &outcomes,
                        active_attempt_id,
                        &track.track_urn,
                        result,
                    );
                }
                is_seeking_flag.store(false, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::{Arc, Mutex}};

    use crate::player::PlaybackOutcome;
    use super::record_playback_result;

    #[test]
    fn playback_results_report_the_attempted_track_and_clear_prior_errors() {
        let error = Arc::new(Mutex::new(None));
        let outcomes = Arc::new(Mutex::new(VecDeque::new()));

        record_playback_result(
            &error,
            &outcomes,
            17,
            "soundcloud:tracks:1",
            Err(anyhow::anyhow!("resolver returned 403")),
        );
        assert_eq!(
            error.lock().unwrap().as_deref(),
            Some("resolver returned 403")
        );
        assert_eq!(
            outcomes.lock().unwrap().pop_front(),
            Some(PlaybackOutcome::Failed {
                attempt_id: 17,
                track_urn: "soundcloud:tracks:1".into(),
                message: "resolver returned 403".into(),
            })
        );

        record_playback_result(
            &error,
            &outcomes,
            18,
            "soundcloud:tracks:2",
            Ok(()),
        );
        assert_eq!(*error.lock().unwrap(), None);
        assert_eq!(
            outcomes.lock().unwrap().pop_front(),
            Some(PlaybackOutcome::Started {
                attempt_id: 18,
                track_urn: "soundcloud:tracks:2".into(),
            })
        );
    }
}
