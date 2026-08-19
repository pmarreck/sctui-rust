use reqwest;

use crate::auth::{Token, try_refresh_token};

use super::super::utils::{
    format_duration, format_playback_count, parse_next_href, parse_str, parse_u64,
};
use crate::api::{API, Artist, Track, get_v2_json};
use std::sync::{Arc, Mutex};

impl API {
    pub fn get_following(&mut self) -> anyhow::Result<Vec<Artist>> {
        let _ = try_refresh_token(&self.token);

        if self.following_next_href.is_none() && !self.first_following_page_fetched {
            self.first_following_page_fetched = true;
        } else if self.following_next_href.is_none() {
            return Ok(Vec::new());
        }

        let user_id = self.ensure_my_user_id()?;
        let token_guard = self.token.lock().unwrap();
        let page = self.following_next_href.clone().unwrap_or_else(|| {
            format!("/users/{user_id}/followings?limit=40&linked_partitioning=true")
        });

        let resp = get_v2_json(
            &reqwest::blocking::Client::new(),
            &self.api_v2_base_url,
            &page,
            &token_guard.access_token,
        )?;

        drop(token_guard);

        self.following_next_href = parse_next_href(&resp);

        let mut following = Vec::new();

        if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
            for artist in collection {
                let name = parse_str(artist, "username");
                let urn = parse_str(artist, "urn");

                following.push(Artist { name, urn });
            }
        }

        Ok(following)
    }
}

fn build_user_tracks_url(user_urn: &str, suffix: &str) -> String {
    let encoded_urn = user_urn.replace(':', "%3A");
    format!(
        "https://api.soundcloud.com/users/{}/{}?linked_partitioning=true&limit=200&access=playable,preview,blocked",
        encoded_urn, suffix
    )
}

pub async fn fetch_following_tracks(
    token: Arc<Mutex<Token>>,
    user_urn: String,
) -> anyhow::Result<Vec<Track>> {
    let _ = try_refresh_token(&token);

    let access_token = { token.lock().unwrap().access_token.clone() };
    let url = build_user_tracks_url(&user_urn, "tracks");

    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            crate::auth::authorization_header(access_token),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let items = if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
        collection.clone()
    } else if let Some(array) = resp.as_array() {
        array.clone()
    } else {
        Vec::new()
    };

    let mut tracks = Vec::new();
    for track in items {
        let title = parse_str(&track, "title");

        let artists = parse_str(&track, "metadata_artist");
        let artists = if !artists.is_empty() {
            artists
        } else {
            parse_str(
                track.get("user").unwrap_or(&serde_json::Value::Null),
                "username",
            )
        };
        let duration = format_duration(parse_u64(&track, "duration"));
        let duration_ms = parse_u64(&track, "duration");

        let playback_count = parse_u64(&track, "playback_count");
        let playback_count = format_playback_count(playback_count);

        let artwork_url = parse_str(&track, "artwork_url");
        let stream_url = parse_str(&track, "stream_url");
        let access = parse_str(&track, "access");
        let track_urn = parse_str(&track, "urn");

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

    Ok(tracks)
}

pub async fn fetch_following_liked_tracks(
    token: Arc<Mutex<Token>>,
    user_urn: String,
) -> anyhow::Result<Vec<Track>> {
    let _ = try_refresh_token(&token);

    let access_token = { token.lock().unwrap().access_token.clone() };
    let url = build_user_tracks_url(&user_urn, "likes/tracks");

    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            crate::auth::authorization_header(access_token),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let items = if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
        collection.clone()
    } else if let Some(array) = resp.as_array() {
        array.clone()
    } else {
        Vec::new()
    };

    let mut tracks = Vec::new();
    for track in items {
        let title = parse_str(&track, "title");

        let artists = parse_str(&track, "metadata_artist");
        let artists = if !artists.is_empty() {
            artists
        } else {
            parse_str(
                track.get("user").unwrap_or(&serde_json::Value::Null),
                "username",
            )
        };
        let duration = format_duration(parse_u64(&track, "duration"));
        let duration_ms = parse_u64(&track, "duration");

        let playback_count = parse_u64(&track, "playback_count");
        let playback_count = format_playback_count(playback_count);

        let artwork_url = parse_str(&track, "artwork_url");
        let stream_url = parse_str(&track, "stream_url");
        let access = parse_str(&track, "access");
        let track_urn = parse_str(&track, "urn");

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

    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use crate::api::API;
    #[test]
    fn browser_session_loads_following_from_api_v2() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let server_observed = Arc::clone(&observed);
        let responder = thread::spawn(move || {
            for _ in 0..2 {
                let request = server
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .expect("following request");
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
                    "/users/9867108/followings?limit=40&linked_partitioning=true" => {
                        r#"{"collection":[{"username":"Followed fixture","urn":"soundcloud:users:42"}]}"#
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

        let token = Arc::new(Mutex::new(
            serde_json::from_value(serde_json::json!({
                "access_token": "browser-token",
                "refresh_token": "refresh",
                "obtained_at": 4_000_000_000_u64
            }))
            .unwrap(),
        ));
        let mut api = API::init_with_api_v2_base_url(token, base_url);
        let following = api.get_following().unwrap();
        responder.join().unwrap();

        assert_eq!(following.len(), 1);
        assert_eq!(following[0].name, "Followed fixture");
        assert_eq!(following[0].urn, "soundcloud:users:42");
        assert_eq!(
            *observed.lock().unwrap(),
            vec![
                ("/me".into(), Some("OAuth browser-token".into())),
                (
                    "/users/9867108/followings?limit=40&linked_partitioning=true".into(),
                    Some("OAuth browser-token".into())
                )
            ]
        );
    }
}
