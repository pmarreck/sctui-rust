mod calls;
mod models;
mod utils;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::auth::Token;
use reqwest::blocking::Client;
use url::Url;

pub use calls::albums::fetch_album_tracks;
pub use calls::engagement::{
    follow_user, like_playlist, like_track, unfollow_user, unlike_playlist, unlike_track,
};
pub use calls::following::{fetch_following_liked_tracks, fetch_following_tracks};
pub use calls::playlists::fetch_playlist_tracks;
pub use calls::search::{
    fetch_search_albums, fetch_search_people, fetch_search_playlists, fetch_search_tracks,
};
pub use models::{Album, Artist, Playlist, Track};

pub struct API {
    token: Arc<Mutex<Token>>,
    api_v2_base_url: String,
    my_user_id: Option<u64>,
    liked_tracks_next_href: Option<String>,
    first_liked_tracks_page_fetched: bool,
    library_items: Option<Vec<serde_json::Value>>,
    library_playlists_delivered: bool,
    library_albums_delivered: bool,
    following_next_href: Option<String>,
    first_following_page_fetched: bool,
}

impl API {
    pub fn init(token: Arc<Mutex<Token>>) -> Self {
        Self::init_with_base_url(token, "https://api-v2.soundcloud.com".into())
    }

    fn init_with_base_url(token: Arc<Mutex<Token>>, api_v2_base_url: String) -> Self {
        Self {
            token,
            api_v2_base_url,
            my_user_id: None,
            liked_tracks_next_href: None,
            first_liked_tracks_page_fetched: false,
            library_items: None,
            library_playlists_delivered: false,
            library_albums_delivered: false,
            following_next_href: None,
            first_following_page_fetched: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn init_with_api_v2_base_url(
        token: Arc<Mutex<Token>>,
        api_v2_base_url: String,
    ) -> Self {
        Self::init_with_base_url(token, api_v2_base_url)
    }

    pub fn token_clone(&self) -> Arc<Mutex<Token>> {
        Arc::clone(&self.token)
    }

    /// Resolves and caches the signed-in numeric user ID required by api-v2's
    /// browser-session-compatible user-scoped collections.
    pub(crate) fn ensure_my_user_id(&mut self) -> anyhow::Result<u64> {
        if let Some(id) = self.my_user_id {
            return Ok(id);
        }

        let access_token = self.token.lock().unwrap().access_token.clone();
        let me = get_v2_json(&Client::new(), &self.api_v2_base_url, "/me", &access_token)?;
        let id = me.get("id").and_then(|value| value.as_u64()).unwrap_or(0);
        if id == 0 {
            anyhow::bail!("signed-in SoundCloud user response had no ID");
        }
        self.my_user_id = Some(id);
        Ok(id)
    }

    /// Loads and caches every api-v2 library page once so playlist and album
    /// views can classify the same response without duplicate network calls.
    pub(crate) fn library_items(&mut self) -> anyhow::Result<Vec<serde_json::Value>> {
        if let Some(items) = &self.library_items {
            return Ok(items.clone());
        }

        let _ = crate::auth::try_refresh_token(&self.token);
        let access_token = self.token.lock().unwrap().access_token.clone();
        let client = Client::new();
        let mut page = "/me/library/all?limit=200&linked_partitioning=true".to_string();
        let mut seen = HashSet::new();
        let mut items = Vec::new();

        while !page.is_empty() {
            if !seen.insert(page.clone()) {
                anyhow::bail!("api-v2 library pagination repeated a page");
            }
            let response = get_v2_json(&client, &self.api_v2_base_url, &page, &access_token)?;
            if let Some(collection) = response
                .get("collection")
                .and_then(|value| value.as_array())
            {
                items.extend(collection.iter().cloned());
            }
            page = response
                .get("next_href")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
        }

        self.library_items = Some(items.clone());
        Ok(items)
    }
}

/// Reads authenticated api-v2 JSON while enforcing same-origin cursors so a
/// recovered browser credential cannot leak through pagination.
pub(crate) fn get_v2_json(
    client: &Client,
    base_url: &str,
    path: &str,
    access_token: &str,
) -> anyhow::Result<serde_json::Value> {
    let base = Url::parse(base_url)?;
    let request_url = base.join(path)?;
    if request_url.scheme() != base.scheme()
        || request_url.host_str() != base.host_str()
        || request_url.port_or_known_default() != base.port_or_known_default()
    {
        anyhow::bail!("api-v2 URL is outside the configured SoundCloud origin");
    }

    Ok(client
        .get(request_url)
        .header(
            reqwest::header::AUTHORIZATION,
            crate::auth::authorization_header(access_token),
        )
        .send()?
        .error_for_status()?
        .json()?)
}
