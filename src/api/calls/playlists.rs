use chrono::{DateTime, FixedOffset, Utc};
use reqwest;
use reqwest::blocking::Client;

use crate::auth::{Token, try_refresh_token};

use super::super::utils::{format_duration, parse_str, parse_track, parse_u64};
use crate::api::{API, Playlist, Track};
use std::sync::{Arc, Mutex};

impl API {
    pub fn get_playlists(&mut self) -> anyhow::Result<Vec<Playlist>> {
        if self.library_playlists_delivered {
            return Ok(Vec::new());
        }

        let items = self.library_items()?;
        let mut playlists = Vec::new();
        for item in items {
            let kind = parse_str(&item, "type");
            if kind != "playlist" && kind != "playlist-like" {
                continue;
            }
            let Some(playlist) = item.get("playlist").filter(|value| !value.is_null()) else {
                continue;
            };
            if parse_str(playlist, "playlist_type").eq_ignore_ascii_case("album") {
                continue;
            }

            let created_at_text = parse_str(playlist, "created_at");
            let created_at = DateTime::parse_from_rfc3339(&created_at_text)
                .or_else(|_| DateTime::parse_from_str(&created_at_text, "%Y/%m/%d %H:%M:%S %z"))
                .unwrap_or_else(|_| Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
            playlists.push(Playlist {
                title: parse_str(playlist, "title"),
                track_count: parse_u64(playlist, "track_count").to_string(),
                duration: format_duration(parse_u64(playlist, "duration")),
                created_at,
                tracks_uri: parse_str(playlist, "tracks_uri"),
                is_owned: kind == "playlist",
            });
        }

        self.library_playlists_delivered = true;
        playlists.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(playlists)
    }

    #[allow(dead_code)]
    pub fn get_playlist_tracks(&mut self, tracks_uri: &str) -> anyhow::Result<Vec<Track>> {
        let _ = try_refresh_token(&self.token);

        let token_guard = self.token.lock().unwrap();

        let mut url = if tracks_uri.starts_with("http") {
            tracks_uri.to_string()
        } else {
            format!("https://api.soundcloud.com{}", tracks_uri)
        };
        if url.contains('?') {
            if !url.contains("linked_partitioning") {
                url.push_str("&linked_partitioning=true");
            }
            if !url.contains("limit=") {
                url.push_str("&limit=200");
            }
        } else {
            url.push_str("?linked_partitioning=true&limit=200");
        }

        let resp: serde_json::Value = Client::new()
            .get(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                crate::auth::authorization_header(&token_guard.access_token),
            )
            .send()?
            .error_for_status()?
            .json()?;

        drop(token_guard);

        let items = if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
            collection.clone()
        } else if let Some(array) = resp.as_array() {
            array.clone()
        } else {
            Vec::new()
        };

        Ok(items.iter().map(parse_track).collect())
    }
}

pub async fn fetch_playlist_tracks(
    token: Arc<Mutex<Token>>,
    tracks_uri: String,
) -> anyhow::Result<Vec<Track>> {
    let _ = try_refresh_token(&token);

    let access_token = { token.lock().unwrap().access_token.clone() };

    let mut url = if tracks_uri.starts_with("http") {
        tracks_uri
    } else {
        format!("https://api.soundcloud.com{}", tracks_uri)
    };
    if url.contains('?') {
        if !url.contains("linked_partitioning") {
            url.push_str("&linked_partitioning=true");
        }
        if !url.contains("limit=") {
            url.push_str("&limit=200");
        }
        if !url.contains("access=") {
            url.push_str("&access=playable,preview,blocked");
        }
    } else {
        url.push_str("?linked_partitioning=true&limit=200&access=playable,preview,blocked");
    }

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

    Ok(items.iter().map(parse_track).collect())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use crate::api::API;
    use crate::auth::Token;

    fn token() -> Arc<Mutex<Token>> {
        Arc::new(Mutex::new(
            serde_json::from_value(serde_json::json!({
                "access_token": "browser-token",
                "refresh_token": "refresh",
                "obtained_at": 4_000_000_000_u64
            }))
            .unwrap(),
        ))
    }

    #[test]
    fn browser_session_loads_playlists_and_albums_from_one_v2_library_page() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let server_observed = Arc::clone(&observed);

        let responder = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .expect("library request");
            let authorization = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string());
            server_observed
                .lock()
                .unwrap()
                .push((request.url().to_string(), authorization));

            request
                .respond(
                    Response::from_string(
                        r#"{
                            "collection": [
                                {"type":"playlist","playlist":{"title":"Owned fixture","playlist_type":"PLAYLIST","track_count":3,"duration":180000,"created_at":"2026-08-19T12:00:00Z","tracks_uri":"/playlists/1/tracks","user":{"username":"owner"}}},
                                {"type":"playlist-like","playlist":{"title":"Liked fixture","playlist_type":"PLAYLIST","track_count":4,"duration":240000,"created_at":"2026-08-18T12:00:00Z","tracks_uri":"/playlists/2/tracks","user":{"username":"other"}}},
                                {"type":"playlist-like","playlist":{"title":"Album fixture","playlist_type":"album","track_count":5,"duration":300000,"created_at":"2026-08-17T12:00:00Z","release_year":2026,"tracks_uri":"/playlists/3/tracks","user":{"username":"artist"}}}
                            ]
                        }"#,
                    )
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap()),
                )
                .unwrap();
        });

        let mut api = API::init_with_api_v2_base_url(token(), base_url);
        let playlists = api.get_playlists().unwrap();
        let albums = api.get_albums().unwrap();
        responder.join().unwrap();

        assert_eq!(
            playlists
                .iter()
                .map(|playlist| (&*playlist.title, playlist.is_owned))
                .collect::<Vec<_>>(),
            vec![("Owned fixture", true), ("Liked fixture", false)]
        );
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].title, "Album fixture");
        assert_eq!(albums[0].artists, "artist");
        assert_eq!(
            *observed.lock().unwrap(),
            vec![(
                "/me/library/all?limit=200&linked_partitioning=true".into(),
                Some("OAuth browser-token".into())
            )]
        );
    }
}
