use crate::auth::Token;

use super::super::utils::{format_duration, parse_str, parse_u64, playlist_tracks_uri};
use crate::api::{API, Album, Track};
use crate::api::calls::playlists::fetch_playlist_tracks;
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
                tracks_uri: playlist_tracks_uri(album),
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
    fetch_playlist_tracks(token, tracks_uri).await
}
