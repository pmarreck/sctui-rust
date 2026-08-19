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
    let stream_url = obj
        .get("media")
        .and_then(|media| media.get("transcodings"))
        .and_then(|value| value.as_array())
        .and_then(|values| select_hls_transcoding_url(values))
        .unwrap_or_else(|| parse_str(obj, "stream_url"));

    crate::api::Track {
        title: parse_str(obj, "title"),
        artists,
        duration: format_duration(duration_ms),
        duration_ms,
        playback_count: format_playback_count(parse_u64(obj, "playback_count")),
        artwork_url: parse_str(obj, "artwork_url"),
        stream_url,
        access: parse_str(obj, "access"),
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
    use super::{parse_track, select_hls_transcoding_url};

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
}
