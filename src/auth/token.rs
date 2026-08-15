use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const REFRESH_TIME: u64 = 2700;

#[derive(Debug, Default, PartialEq, Eq)]
enum TokenSource {
    #[default]
    Application,
    Firefox(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,

    #[serde(default)]
    pub obtained_at: u64,

    #[serde(skip)]
    source: TokenSource,

    #[serde(skip, default = "refresh_enabled_by_default")]
    refresh_enabled: bool,
}

fn refresh_enabled_by_default() -> bool {
    true
}

/// Formats SoundCloud's API-specific OAuth authorization scheme from any
/// access-token string without coupling request code to token storage.
pub fn authorization_header(access_token: impl AsRef<str>) -> String {
    format!("OAuth {}", access_token.as_ref())
}

impl Token {
    /// Represents Firefox's browser-session cookie without pretending it can use
    /// SoundCloud's application refresh-token flow.
    pub fn from_firefox(access_token: String, profile: String) -> Self {
        Self {
            access_token,
            refresh_token: String::new(),
            obtained_at: 0,
            source: TokenSource::Firefox(profile),
            refresh_enabled: false,
        }
    }

    /// Formats the authorization scheme required by SoundCloud's API.
    pub fn authorization_header(&self) -> String {
        authorization_header(&self.access_token)
    }

    pub fn is_refreshable(&self) -> bool {
        self.refresh_enabled
            && self.source == TokenSource::Application
            && !self.refresh_token.is_empty()
    }

    pub fn disable_refresh(&mut self) {
        self.refresh_enabled = false;
    }

    pub fn source_label(&self) -> String {
        match &self.source {
            TokenSource::Application => "SoundCloud OAuth".into(),
            TokenSource::Firefox(profile) => format!("Firefox ({profile})"),
        }
    }

    /// Decides refresh eligibility from an injected timestamp so callers and
    /// tests cannot accidentally refresh browser or otherwise disabled tokens.
    pub(crate) fn should_refresh_at(&self, now: u64) -> bool {
        self.is_refreshable() && self.is_expired_at(now)
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.is_expired_at(now)
    }

    fn is_expired_at(&self, now: u64) -> bool {
        if matches!(self.source, TokenSource::Firefox(_)) {
            return false;
        }
        now > self.obtained_at + REFRESH_TIME
    }
}

pub fn load_token() -> Option<Token> {
    let data = fs::read_to_string("token.json").ok()?;
    let token: Token = serde_json::from_str(&data).ok()?;
    if token.is_expired() {
        None
    } else {
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::Token;

    #[test]
    fn soundcloud_authorization_uses_oauth_scheme() {
        let token = Token {
            access_token: "secret-access-token".into(),
            refresh_token: "refresh-token".into(),
            obtained_at: 42,
            source: Default::default(),
            refresh_enabled: true,
        };

        assert_eq!(token.authorization_header(), "OAuth secret-access-token");
    }

    #[test]
    fn firefox_tokens_are_non_refreshable_and_do_not_expire_locally() {
        let token = Token::from_firefox("browser-token".into(), "default-release".into());

        assert!(!token.is_refreshable());
        assert!(!token.is_expired());
        assert_eq!(token.source_label(), "Firefox (default-release)");
    }

    #[test]
    fn disabled_refresh_prevents_an_expired_application_token_from_refreshing() {
        let mut token = Token {
            access_token: "saved-token".into(),
            refresh_token: "refresh-token".into(),
            obtained_at: 100,
            source: Default::default(),
            refresh_enabled: true,
        };

        assert!(token.should_refresh_at(100 + super::REFRESH_TIME + 1));
        token.disable_refresh();
        assert!(!token.should_refresh_at(100 + super::REFRESH_TIME + 1));
    }
}
