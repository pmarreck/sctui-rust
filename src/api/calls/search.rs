use chrono::{DateTime, FixedOffset, Utc};

use crate::auth::{Token, try_refresh_token};

use super::super::utils::{
    format_duration, is_album, parse_str, parse_track, parse_u64, playlist_tracks_uri,
};
use crate::api::{API_V2_BASE_URL, Album, Artist, Playlist, Track, get_v2_json_async};
use std::sync::{Arc, Mutex};

fn parse_created_at(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_str(value, "%Y/%m/%d %H:%M:%S %z"))
        .unwrap_or_else(|_| Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()))
}

fn response_items(resp: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(collection) = resp.get("collection").and_then(|v| v.as_array()) {
        collection.clone()
    } else if let Some(array) = resp.as_array() {
        array.clone()
    } else {
        Vec::new()
    }
}

fn search_path(resource: &str, query: &str, include_access: bool) -> String {
    let mut parameters = url::form_urlencoded::Serializer::new(String::new());
    parameters.append_pair("q", query);
    parameters.append_pair("linked_partitioning", "true");
    parameters.append_pair("limit", "50");
    if include_access {
        parameters.append_pair("access", "playable,preview,blocked");
    }
    format!("/search/{resource}?{}", parameters.finish())
}

/// Searches one api-v2 resource while keeping browser credentials on the configured origin.
async fn fetch_search_json(
    token: Arc<Mutex<Token>>,
    query: String,
    api_v2_base_url: String,
    resource: &str,
    include_access: bool,
) -> anyhow::Result<serde_json::Value> {
    let _ = try_refresh_token(&token);
    let access_token = { token.lock().unwrap().access_token.clone() };
    get_v2_json_async(
        &api_v2_base_url,
        &search_path(resource, &query, include_access),
        &access_token,
    )
    .await
}

pub async fn fetch_search_tracks(
    token: Arc<Mutex<Token>>,
    query: String,
) -> anyhow::Result<Vec<Track>> {
    fetch_search_tracks_from_base(token, query, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_search_tracks_from_base(
    token: Arc<Mutex<Token>>,
    query: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Track>> {
    let resp = fetch_search_json(token, query, api_v2_base_url, "tracks", true).await?;

    Ok(response_items(&resp)
        .into_iter()
        .map(|v| parse_track(&v))
        .collect())
}

pub async fn fetch_search_albums(
    token: Arc<Mutex<Token>>,
    query: String,
) -> anyhow::Result<Vec<Album>> {
    fetch_search_albums_from_base(token, query, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_search_albums_from_base(
    token: Arc<Mutex<Token>>,
    query: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Album>> {
    let resp = fetch_search_json(token, query, api_v2_base_url, "albums", false).await?;

    let mut albums = Vec::new();
    for playlist in response_items(&resp) {
        if !is_album(&playlist) {
            continue;
        }

        let title = parse_str(&playlist, "title");
        let artists = parse_str(
            playlist.get("user").unwrap_or(&serde_json::Value::Null),
            "username",
        );
        let release_year = parse_u64(&playlist, "release_year").to_string();
        let track_count = parse_u64(&playlist, "track_count").to_string();
        let duration = format_duration(parse_u64(&playlist, "duration"));
        let tracks_uri = playlist_tracks_uri(&playlist);

        albums.push(Album {
            title,
            artists,
            release_year,
            duration,
            track_count,
            tracks_uri,
        });
    }

    Ok(albums)
}

pub async fn fetch_search_playlists(
    token: Arc<Mutex<Token>>,
    query: String,
) -> anyhow::Result<Vec<Playlist>> {
    fetch_search_playlists_from_base(token, query, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_search_playlists_from_base(
    token: Arc<Mutex<Token>>,
    query: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Playlist>> {
    let resp = fetch_search_json(
        token,
        query,
        api_v2_base_url,
        "playlists_without_albums",
        false,
    )
    .await?;

    let mut playlists = Vec::new();
    for playlist in response_items(&resp) {
        if is_album(&playlist) {
            continue;
        }

        let title = parse_str(&playlist, "title");
        let track_count = parse_u64(&playlist, "track_count").to_string();
        let duration = format_duration(parse_u64(&playlist, "duration"));
        let created_at = parse_created_at(&parse_str(&playlist, "created_at"));
        let tracks_uri = playlist_tracks_uri(&playlist);

        playlists.push(Playlist {
            title,
            track_count,
            duration,
            created_at,
            tracks_uri,
            is_owned: false,
        });
    }

    playlists.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(playlists)
}

pub async fn fetch_search_people(
    token: Arc<Mutex<Token>>,
    query: String,
) -> anyhow::Result<Vec<Artist>> {
    fetch_search_people_from_base(token, query, API_V2_BASE_URL.into()).await
}

pub(crate) async fn fetch_search_people_from_base(
    token: Arc<Mutex<Token>>,
    query: String,
    api_v2_base_url: String,
) -> anyhow::Result<Vec<Artist>> {
    let resp = fetch_search_json(token, query, api_v2_base_url, "users", false).await?;

    let mut people = Vec::new();
    for user in response_items(&resp) {
        let name = parse_str(&user, "username");
        let urn = parse_str(&user, "urn");
        if name.is_empty() || urn.is_empty() {
            continue;
        }
        people.push(Artist { name, urn });
    }

    Ok(people)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use tiny_http::{Header, Response, Server};

    use crate::auth::Token;

    use super::{
        fetch_search_albums_from_base, fetch_search_people_from_base,
        fetch_search_playlists_from_base, fetch_search_tracks_from_base,
    };

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

    #[tokio::test(flavor = "current_thread")]
    async fn browser_search_uses_api_v2_resources_for_every_filter() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let responder = thread::spawn(move || {
            let fixtures = [
                (
                    "/search/tracks?q=SoundCloud&linked_partitioning=true&limit=50&access=playable%2Cpreview%2Cblocked",
                    r#"{"collection":[{"title":"Track fixture","duration":180000,"urn":"soundcloud:tracks:1","access":"playable","user":{"username":"artist"}}]}"#,
                ),
                (
                    "/search/albums?q=SoundCloud&linked_partitioning=true&limit=50",
                    r#"{"collection":[{"id":2,"title":"Album fixture","is_album":true,"set_type":"album","track_count":2,"duration":180000,"release_year":2026,"user":{"username":"artist"}}]}"#,
                ),
                (
                    "/search/playlists_without_albums?q=SoundCloud&linked_partitioning=true&limit=50",
                    r#"{"collection":[{"id":3,"title":"Playlist fixture","playlist_type":"playlist","track_count":3,"duration":180000,"created_at":"2026-08-20T10:00:00Z"}]}"#,
                ),
                (
                    "/search/users?q=SoundCloud&linked_partitioning=true&limit=50",
                    r#"{"collection":[{"username":"Person fixture","urn":"soundcloud:users:4"}]}"#,
                ),
            ];

            for (expected, body) in fixtures {
                let request = server
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .expect("search request");
                assert_eq!(request.url(), expected);
                request
                    .respond(Response::from_string(body).with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ))
                    .unwrap();
            }
        });

        let tracks = fetch_search_tracks_from_base(token(), "SoundCloud".into(), base_url.clone())
            .await
            .unwrap();
        let albums = fetch_search_albums_from_base(token(), "SoundCloud".into(), base_url.clone())
            .await
            .unwrap();
        let playlists =
            fetch_search_playlists_from_base(token(), "SoundCloud".into(), base_url.clone())
                .await
                .unwrap();
        let people = fetch_search_people_from_base(token(), "SoundCloud".into(), base_url)
            .await
            .unwrap();
        responder.join().unwrap();

        assert_eq!(tracks[0].title, "Track fixture");
        assert_eq!(albums[0].tracks_uri, "/playlists/2/tracks");
        assert_eq!(playlists[0].tracks_uri, "/playlists/3/tracks");
        assert_eq!(people[0].name, "Person fixture");
    }

    #[test]
    #[ignore = "uses Peter's live Firefox session and SoundCloud search"]
    fn live_browser_session_search_returns_every_result_type() {
        let token = Arc::new(Mutex::new(crate::auth::initial_token().unwrap()));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let query = "Taylor Swift".to_string();

        let tracks = runtime
            .block_on(super::fetch_search_tracks(
                Arc::clone(&token),
                query.clone(),
            ))
            .unwrap();
        let albums = runtime
            .block_on(super::fetch_search_albums(
                Arc::clone(&token),
                query.clone(),
            ))
            .unwrap();
        let playlists = runtime
            .block_on(super::fetch_search_playlists(
                Arc::clone(&token),
                query.clone(),
            ))
            .unwrap();
        let people = runtime
            .block_on(super::fetch_search_people(token, query))
            .unwrap();

        assert!(!tracks.is_empty());
        assert!(!albums.is_empty());
        assert!(!playlists.is_empty());
        assert!(!people.is_empty());
    }
}
