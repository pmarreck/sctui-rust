pub(crate) fn format_playback_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.2}K", n as f64 / 1_000.0),
        1_000_000..=999_999_999 => format!("{:.2}M", n as f64 / 1_000_000.0),
        1_000_000_000..=999_999_999_999 => format!("{:.2}B", n as f64 / 1_000_000_000.0),
        _ => format!("{:.2}T", n as f64 / 1_000_000_000_000.0),
    }
}

pub(crate) fn format_duration(duration_ms: u64) -> String {
    let duration_sec = duration_ms / 1000;
    let hours = duration_sec / 3600;
    let minutes = (duration_sec % 3600) / 60;
    let seconds = duration_sec % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

pub(crate) fn parse_str(obj: &serde_json::Value, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn parse_u64(obj: &serde_json::Value, key: &str) -> u64 {
    obj.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// Supplies the nested-track path omitted by current api-v2 playlist objects.
pub(crate) fn playlist_tracks_uri(playlist: &serde_json::Value) -> String {
    let existing = parse_str(playlist, "tracks_uri");
    if !existing.is_empty() {
        return existing;
    }

    let id = parse_u64(playlist, "id");
    let id = if id > 0 {
        Some(id)
    } else {
        parse_str(playlist, "urn")
            .rsplit(':')
            .next()
            .and_then(|value| value.parse().ok())
    };
    id.map(|id| format!("/playlists/{id}/tracks"))
        .unwrap_or_default()
}

/// Recognizes album discriminators returned by both legacy and api-v2 playlist objects.
pub(crate) fn is_album(playlist: &serde_json::Value) -> bool {
    playlist
        .get("is_album")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || parse_str(playlist, "set_type").eq_ignore_ascii_case("album")
        || parse_str(playlist, "playlist_type").eq_ignore_ascii_case("album")
}

/// Selects the preferred non-DRM HLS resolver from a complete transcoding set.
/// MPEG audio is preferred because the playback engine already decodes it reliably.
pub(crate) fn select_hls_transcoding_url(transcodings: &[serde_json::Value]) -> Option<String> {
    for prefer_ogg in [false, true] {
        for transcoding in transcodings {
            let Some(format) = transcoding.get("format") else {
                continue;
            };
            let protocol = parse_str(format, "protocol").to_ascii_lowercase();
            if protocol.starts_with("cbc-") || protocol.starts_with("ctr-") {
                continue;
            }
            if protocol != "hls" && protocol != "encrypted-hls" {
                continue;
            }
            let is_ogg = parse_str(format, "mime_type")
                .to_ascii_lowercase()
                .contains("ogg");
            if is_ogg == prefer_ogg {
                let url = parse_str(transcoding, "url");
                if !url.is_empty() {
                    return Some(url);
                }
            }
        }
    }
    None
}

/// Classifies service-policy and codec boundaries from the complete track
/// response so dead legacy resolver URLs cannot hide known restrictions.
fn classify_playback_restriction(
    obj: &serde_json::Value,
    transcodings: &[serde_json::Value],
    stream_url: &str,
) -> Option<crate::api::PlaybackRestriction> {
    use crate::api::PlaybackRestriction;

    let policy = parse_str(obj, "policy").to_ascii_uppercase();
    let monetization = parse_str(obj, "monetization_model").to_ascii_uppercase();
    if policy == "MONETIZE" && monetization == "SUB_HIGH_TIER" {
        return Some(PlaybackRestriction::SoundCloudGoPlus);
    }

    match parse_str(obj, "access").to_ascii_lowercase().as_str() {
        "blocked" => return Some(PlaybackRestriction::Blocked),
        "preview" => return Some(PlaybackRestriction::PreviewOnly),
        _ => {}
    }

    // An obsolete stream_url must not override an explicit encrypted-only set.
    if stream_url.is_empty()
        || (!transcodings.is_empty() && select_hls_transcoding_url(transcodings).is_none())
    {
        let has_encrypted_stream = transcodings.iter().any(|transcoding| {
            let protocol = transcoding
                .get("format")
                .map(|format| parse_str(format, "protocol"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            protocol.starts_with("cbc-") || protocol.starts_with("ctr-")
        });
        if has_encrypted_stream {
            return Some(PlaybackRestriction::EncryptedStream);
        }
        if stream_url.is_empty() {
            return Some(PlaybackRestriction::Unavailable);
        }
    }

    None
}

/// Converts SoundCloud track JSON into the shared UI model and retains its
/// preferred API-v2 transcoding resolver for playback.
pub(crate) fn parse_track(obj: &serde_json::Value) -> crate::api::Track {
    let artists = parse_str(obj, "metadata_artist");
    let artists = if artists.is_empty() {
        parse_str(
            obj.get("user").unwrap_or(&serde_json::Value::Null),
            "username",
        )
    } else {
        artists
    };
    let duration_ms = parse_u64(obj, "duration");
    let transcodings = obj
        .get("media")
        .and_then(|media| media.get("transcodings"))
        .and_then(|value| value.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let stream_url = select_hls_transcoding_url(transcodings)
        .unwrap_or_else(|| parse_str(obj, "stream_url"));
    let playback_restriction = classify_playback_restriction(obj, transcodings, &stream_url);

    crate::api::Track {
        title: parse_str(obj, "title"),
        artists,
        duration: format_duration(duration_ms),
        duration_ms,
        playback_count: format_playback_count(parse_u64(obj, "playback_count")),
        artwork_url: parse_str(obj, "artwork_url"),
        stream_url,
        access: parse_str(obj, "access"),
        playback_restriction,
        track_urn: parse_str(obj, "urn"),
    }
}

pub(crate) fn parse_next_href(resp: &serde_json::Value) -> Option<String> {
    resp.get("next_href")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use crate::api::PlaybackRestriction;

    use super::{is_album, parse_track, select_hls_transcoding_url};

    #[test]
    fn album_classifier_covers_legacy_and_api_v2_discriminator_sets() {
        let candidates = [
            serde_json::json!({"playlist_type":"album"}),
            serde_json::json!({"set_type":"ALBUM"}),
            serde_json::json!({"is_album":true}),
            serde_json::json!({"playlist_type":"playlist"}),
            serde_json::json!({"set_type":"playlist","is_album":false}),
            serde_json::json!({}),
        ];

        assert_eq!(
            candidates.iter().map(is_album).collect::<Vec<_>>(),
            vec![true, true, true, false, false, false]
        );
    }

    #[test]
    fn hls_selection_classifies_the_complete_candidate_set() {
        let transcodings = serde_json::json!([
            {"url":"https://resolver/malformed"},
            {"url":"https://resolver/progressive","format":{"protocol":"progressive","mime_type":"audio/mpeg"}},
            {"url":"https://resolver/drm","format":{"protocol":"cbc-hls","mime_type":"audio/mpeg"}},
            {"url":"https://resolver/ogg","format":{"protocol":"hls","mime_type":"audio/ogg; codecs=\"opus\""}},
            {"url":"https://resolver/mp3","format":{"protocol":"hls","mime_type":"audio/mpeg"}}
        ]);

        assert_eq!(
            select_hls_transcoding_url(transcodings.as_array().unwrap()),
            Some("https://resolver/mp3".to_string())
        );
    }

    #[test]
    fn track_parser_uses_selected_transcoding_as_its_stream_url() {
        let track = parse_track(&serde_json::json!({
            "title":"Playable fixture",
            "duration":123000,
            "urn":"soundcloud:tracks:404",
            "stream_url":"https://legacy/stream",
            "media":{"transcodings":[
                {"url":"https://resolver/hls","format":{"protocol":"hls","mime_type":"audio/mpeg"}}
            ]}
        }));

        assert_eq!(track.stream_url, "https://resolver/hls");
    }

    #[test]
    fn playback_restrictions_classify_complete_track_sets() {
        let cases = [
            (
                serde_json::json!({
                    "access": null,
                    "policy": "MONETIZE",
                    "monetization_model": "SUB_HIGH_TIER",
                    "media": {"transcodings": [
                        {"url":"https://resolver/encrypted","format":{"protocol":"cbc-encrypted-hls","mime_type":"audio/mp4"}},
                        {"url":"https://resolver/dead-legacy","format":{"protocol":"hls","mime_type":"audio/mpeg"}}
                    ]}
                }),
                Some(PlaybackRestriction::SoundCloudGoPlus),
            ),
            (
                serde_json::json!({"access":"blocked"}),
                Some(PlaybackRestriction::Blocked),
            ),
            (
                serde_json::json!({"access":"preview"}),
                Some(PlaybackRestriction::PreviewOnly),
            ),
            (
                serde_json::json!({
                    "access":"playable",
                    "media":{"transcodings":[
                        {"url":"https://resolver/encrypted","format":{"protocol":"ctr-encrypted-hls","mime_type":"audio/mp4"}}
                    ]}
                }),
                Some(PlaybackRestriction::EncryptedStream),
            ),
            (
                serde_json::json!({
                    "access":"playable",
                    "media":{"transcodings":[
                        {"url":"https://resolver/hls","format":{"protocol":"hls","mime_type":"audio/mpeg"}}
                    ]}
                }),
                None,
            ),
            (
                serde_json::json!({"access":"playable"}),
                Some(PlaybackRestriction::Unavailable),
            ),
        ];

        let observed = cases
            .iter()
            .map(|(value, _)| parse_track(value).playback_restriction)
            .collect::<Vec<_>>();
        let expected = cases
            .iter()
            .map(|(_, expected)| *expected)
            .collect::<Vec<_>>();

        assert_eq!(observed, expected);
    }

    #[test]
    fn encrypted_only_metadata_overrides_a_legacy_stream_url() {
        let observed = [
            vec!["cbc-encrypted-hls"],
            vec!["ctr-encrypted-hls"],
            vec!["hls"],
            vec!["cbc-encrypted-hls", "hls"],
            vec!["progressive"],
        ]
            .map(|protocols| parse_track(&serde_json::json!({
                "stream_url":"https://legacy/stream",
                "media":{"transcodings":protocols.iter().map(|protocol| serde_json::json!({"url":"https://resolver/audio",
                    "format":{"protocol":protocol,"mime_type":"audio/mpeg"}})).collect::<Vec<_>>()}
            })).playback_restriction);
        assert_eq!(observed, [
            Some(PlaybackRestriction::EncryptedStream),
            Some(PlaybackRestriction::EncryptedStream),
            None,
            None,
            None,
        ]);
    }
}
