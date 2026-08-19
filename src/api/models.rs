use chrono::{DateTime, FixedOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackRestriction {
    SoundCloudGoPlus,
    PreviewOnly,
    Blocked,
    EncryptedStream,
    Unavailable,
}

impl PlaybackRestriction {
    /// Explains the strongest policy evidence supplied with a track without
    /// presenting SoundCloud's deliberately masked resolver status as a missing file.
    pub fn message(self) -> &'static str {
        match self {
            Self::SoundCloudGoPlus => {
                "SoundCloud Go+ track requires encrypted high-tier playback, which sctui does not support"
            }
            Self::PreviewOnly => "SoundCloud permits only preview playback for this track",
            Self::Blocked => {
                "SoundCloud reports this track as blocked by creator, paywall, or region policy"
            }
            Self::EncryptedStream => {
                "SoundCloud supplied only encrypted streams, which sctui does not support"
            }
            Self::Unavailable => "SoundCloud supplied no playable stream for this track",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub title: String,
    pub artists: String,
    pub duration: String,
    pub duration_ms: u64,
    pub playback_count: String,
    pub artwork_url: String,
    pub stream_url: String,
    pub access: String,
    pub playback_restriction: Option<PlaybackRestriction>,
    pub track_urn: String,
}

impl Track {
    pub fn is_playable(&self) -> bool {
        self.playback_restriction.is_none()
            && (self.access.is_empty() || self.access == "playable")
    }
}

#[derive(Debug, Clone)]
pub struct Playlist {
    pub title: String,
    pub track_count: String,
    pub duration: String,
    pub created_at: DateTime<FixedOffset>,
    pub tracks_uri: String,
    /// True if this playlist comes from `/me/playlists` (owned by the logged-in user).
    /// False if it comes from `/me/likes/playlists` (liked playlists).
    pub is_owned: bool,
}

#[derive(Debug, Clone)]
pub struct Album {
    pub title: String,
    pub artists: String,
    pub release_year: String,
    pub duration: String,
    pub track_count: String,
    pub tracks_uri: String,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub name: String,
    pub urn: String,
}
