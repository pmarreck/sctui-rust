use reqwest::blocking::Client;

use crate::auth::try_refresh_token;

use super::super::utils::{
    format_duration, format_playback_count, parse_next_href, parse_str, parse_u64,
};
use crate::api::{API, Track, get_v2_json};

impl API {
    pub fn get_liked_tracks(&mut self) -> anyhow::Result<Vec<Track>> {
        let _ = try_refresh_token(&self.token);

        if self.first_liked_tracks_page_fetched && self.liked_tracks_next_href.is_none() {
            return Ok(Vec::new());
        }

        let client = Client::new();
        let user_id = self.ensure_my_user_id()?;
        let token_guard = self.token.lock().unwrap();

        let page = self.liked_tracks_next_href.clone().unwrap_or_else(|| {
            format!(
                "/users/{}/track_likes?limit=40&linked_partitioning=true",
                user_id
            )
        });
        let resp = get_v2_json(
            &client,
            &self.api_v2_base_url,
            &page,
            &token_guard.access_token,
        )?;

        drop(token_guard);

        self.liked_tracks_next_href = parse_next_href(&resp);
        self.first_liked_tracks_page_fetched = true;

        let mut tracks = Vec::new();

        if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
            for item in collection {
                let Some(track) = item.get("track").filter(|track| !track.is_null()) else {
                    continue;
                };
                let title = parse_str(track, "title");

                let artists = parse_str(track, "metadata_artist");
                let artists = if !artists.is_empty() {
                    artists
                } else {
                    parse_str(
                        track.get("user").unwrap_or(&serde_json::Value::Null),
                        "username",
                    )
                };
                let duration = format_duration(parse_u64(track, "duration"));
                let duration_ms = parse_u64(track, "duration");

                let playback_count = parse_u64(track, "playback_count");
                let playback_count = format_playback_count(playback_count);

                let artwork_url = parse_str(track, "artwork_url");

                let stream_url = parse_str(track, "stream_url");
                let access = parse_str(track, "access");
                let track_urn = parse_str(track, "urn");

                tracks.push(Track {
                    title,
                    artists,
                    duration,
                    duration_ms,
                    playback_count,
                    artwork_url,
                    stream_url,
                    access,
                    track_urn,
                });
            }
        }

        Ok(tracks)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use crate::api::API;
    use crate::auth::Token;

    fn token(value: &str) -> Arc<Mutex<Token>> {
        Arc::new(Mutex::new(
            serde_json::from_value(serde_json::json!({
                "access_token": value,
                "refresh_token": "refresh",
                "obtained_at": 4_000_000_000_u64
            }))
            .unwrap(),
        ))
    }

    #[test]
    fn browser_session_uses_v2_user_track_likes_instead_of_forbidden_legacy_route() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let server_observed = Arc::clone(&observed);

        let responder = thread::spawn(move || {
            for _ in 0..2 {
                let Some(request) = server.recv_timeout(Duration::from_secs(2)).unwrap() else {
                    break;
                };
                let authorization = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("Authorization"))
                    .map(|header| header.value.as_str().to_string());
                server_observed
                    .lock()
                    .unwrap()
                    .push((request.url().to_string(), authorization));

                let body = match request.url() {
                    "/me" => r#"{"id":9867108}"#,
                    "/users/9867108/track_likes?limit=40&linked_partitioning=true" => {
                        r#"{"collection":[{"track":{"title":"Liked fixture","duration":184000,"urn":"soundcloud:tracks:404","access":"playable","user":{"username":"fixture artist"}}}]}"#
                    }
                    _ => {
                        request.respond(Response::empty(403)).unwrap();
                        continue;
                    }
                };
                request
                    .respond(Response::from_string(body).with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ))
                    .unwrap();
            }
        });

        let mut api = API::init_with_api_v2_base_url(token("browser-token"), base_url);
        let tracks = api.get_liked_tracks().unwrap();
        responder.join().unwrap();

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Liked fixture");
        assert_eq!(tracks[0].artists, "fixture artist");
        assert_eq!(
            *observed.lock().unwrap(),
            vec![
                ("/me".into(), Some("OAuth browser-token".into())),
                (
                    "/users/9867108/track_likes?limit=40&linked_partitioning=true".into(),
                    Some("OAuth browser-token".into())
                ),
            ]
        );
    }

    #[test]
    fn v2_pagination_rejects_cross_origin_urls_before_sending_credentials() {
        let error = crate::api::get_v2_json(
            &reqwest::blocking::Client::new(),
            "https://api-v2.soundcloud.com",
            "https://example.invalid/track_likes?cursor=next",
            "browser-token",
        )
        .unwrap_err();

        assert!(error.to_string().contains("outside"));
    }
}
