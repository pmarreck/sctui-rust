use crate::auth::{Token, try_refresh_token};

use super::super::utils::{parse_next_href, parse_str, parse_track};
use crate::api::{
    API, API_V2_BASE_URL, Artist, Track, append_track_collection_query, get_v2_json,
    get_v2_json_async,
};
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

fn user_id_from_urn(user_urn: &str) -> anyhow::Result<u64> {
    user_urn
        .rsplit(':')
        .next()
        .and_then(|value| value.parse().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| anyhow::anyhow!("SoundCloud user URN had no numeric ID"))
}

pub async fn fetch_following_tracks(
    token: Arc<Mutex<Token>>,
    user_urn: String,
) -> anyhow::Result<Vec<Track>> {
    fetch_following_tracks_from_base(token, user_urn, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_following_tracks_from_base(
    token: Arc<Mutex<Token>>,
    user_urn: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Track>> {
    fetch_user_track_collection(token, user_urn, api_v2_base_url, "tracks", false).await
}

async fn fetch_user_track_collection(
    token: Arc<Mutex<Token>>,
    user_urn: String,
    api_v2_base_url: String,
    suffix: &str,
    wrapped: bool,
) -> anyhow::Result<Vec<Track>> {
    let _ = try_refresh_token(&token);
    let access_token = { token.lock().unwrap().access_token.clone() };
    let path = append_track_collection_query(format!(
        "/users/{}/{}",
        user_id_from_urn(&user_urn)?,
        suffix
    ));
    let resp = get_v2_json_async(&api_v2_base_url, &path, &access_token).await?;

    let items = if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
        collection.clone()
    } else if let Some(array) = resp.as_array() {
        array.clone()
    } else {
        Vec::new()
    };

    Ok(items
        .iter()
        .filter_map(|item| {
            if wrapped {
                item.get("track")
            } else {
                Some(item)
            }
        })
        .map(parse_track)
        .collect())
}

pub async fn fetch_following_liked_tracks(
    token: Arc<Mutex<Token>>,
    user_urn: String,
) -> anyhow::Result<Vec<Track>> {
    fetch_following_likes_from_base(token, user_urn, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_following_likes_from_base(
    token: Arc<Mutex<Token>>,
    user_urn: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Track>> {
    fetch_user_track_collection(token, user_urn, api_v2_base_url, "track_likes", true).await
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use crate::api::API;
    use crate::auth::Token;

    use super::{fetch_following_likes_from_base, fetch_following_tracks_from_base};

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

    #[tokio::test(flavor = "current_thread")]
    async fn nested_following_tracks_use_numeric_v2_routes_and_parse_like_wrappers() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let responder = thread::spawn(move || {
            for expected in [
                "/users/42/tracks?linked_partitioning=true&limit=200&access=playable,preview,blocked",
                "/users/42/track_likes?linked_partitioning=true&limit=200&access=playable,preview,blocked",
            ] {
                let request = server
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .expect("following tracks request");
                assert_eq!(request.url(), expected);
                let body = if expected.contains("track_likes") {
                    r#"{"collection":[{"track":{"title":"Liked fixture","duration":180000,"urn":"soundcloud:tracks:2","access":"playable","user":{"username":"fixture"}}}]}"#
                } else {
                    r#"{"collection":[{"title":"Published fixture","duration":180000,"urn":"soundcloud:tracks:1","access":"playable","user":{"username":"fixture"}}]}"#
                };
                request
                    .respond(
                        Response::from_string(body).with_header(
                            Header::from_bytes("Content-Type", "application/json").unwrap(),
                        ),
                    )
                    .unwrap();
            }
        });

        let published = fetch_following_tracks_from_base(
            token(),
            "soundcloud:users:42".into(),
            base_url.clone(),
        )
        .await
        .unwrap();
        let liked = fetch_following_likes_from_base(
            token(),
            "soundcloud:users:42".into(),
            base_url,
        )
        .await
        .unwrap();
        responder.join().unwrap();

        assert_eq!(published[0].title, "Published fixture");
        assert_eq!(liked[0].title, "Liked fixture");
    }

    #[test]
    #[ignore = "uses Peter's live Firefox session and SoundCloud library"]
    fn live_browser_session_loads_tracks_for_a_followed_person() {
        let token = Arc::new(Mutex::new(crate::auth::initial_token().unwrap()));
        let mut api = API::init(Arc::clone(&token));
        let followed = api.get_following().unwrap();
        let mut found_tracks = false;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        for artist in followed.into_iter().take(5) {
            let published = runtime
                .block_on(super::fetch_following_tracks(
                    Arc::clone(&token),
                    artist.urn.clone(),
                ))
                .unwrap();
            let liked = runtime
                .block_on(super::fetch_following_liked_tracks(
                    Arc::clone(&token),
                    artist.urn,
                ))
                .unwrap();
            if !published.is_empty() || !liked.is_empty() {
                found_tracks = true;
                break;
            }
        }

        assert!(found_tracks, "first five followed people had no visible tracks");
    }
}
