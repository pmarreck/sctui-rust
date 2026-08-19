use reqwest;

use crate::auth::{Token, try_refresh_token};

use super::super::utils::{format_duration, parse_str, parse_track, parse_u64};
use crate::api::{API, Album, Track};
use std::sync::{Arc, Mutex};

impl API {
    pub fn get_albums(&mut self) -> anyhow::Result<Vec<Album>> {
        if self.library_albums_delivered {
            return Ok(Vec::new());
        }

        let items = self.library_items()?;
        let mut albums = Vec::new();
        for item in items {
            let Some(album) = item.get("playlist").filter(|value| !value.is_null()) else {
                continue;
            };
            if !parse_str(album, "playlist_type").eq_ignore_ascii_case("album") {
                continue;
            }

            albums.push(Album {
                title: parse_str(album, "title"),
                artists: parse_str(
                    album.get("user").unwrap_or(&serde_json::Value::Null),
                    "username",
                ),
                release_year: parse_u64(album, "release_year").to_string(),
                track_count: parse_u64(album, "track_count").to_string(),
                duration: format_duration(parse_u64(album, "duration")),
                tracks_uri: parse_str(album, "tracks_uri"),
            });
        }

        self.library_albums_delivered = true;
        Ok(albums)
    }
}

pub async fn fetch_album_tracks(
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
