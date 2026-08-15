mod firefox;
mod oauth;
mod refresh;
mod token;

use anyhow::{Result, anyhow};

pub use oauth::authenticate;
#[allow(unused_imports)]
pub use refresh::{refresh_token, start_auto_refresh, try_refresh_token};
pub use token::{Token, authorization_header, load_token};

/// Acquires startup authentication without touching lower-precedence adapters,
/// so Firefox is inspected only when saved and application OAuth are unavailable.
fn resolve_initial_token<Authorize, FindFirefox>(
    saved: Option<Token>,
    credentials: Option<oauth::ApplicationCredentials>,
    authorize: Authorize,
    find_firefox: FindFirefox,
) -> Result<Token>
where
    Authorize: FnOnce(oauth::ApplicationCredentials) -> Result<Token>,
    FindFirefox: FnOnce() -> Option<Token>,
{
    if let Some(mut token) = saved {
        if credentials.is_none() {
            token.disable_refresh();
        }
        return Ok(token);
    }
    if let Some(credentials) = credentials {
        return authorize(credentials);
    }
    find_firefox().ok_or_else(|| {
        anyhow!(
            "no Firefox SoundCloud session found; set SOUNDCLOUD_CLIENT_ID and \
             SOUNDCLOUD_CLIENT_SECRET or sign in to SoundCloud in Firefox"
        )
    })
}

pub fn initial_token() -> Result<Token> {
    let saved = load_token();
    let credentials = oauth::ApplicationCredentials::from_environment()?;
    resolve_initial_token(
        saved,
        credentials,
        |credentials| oauth::authenticate_with(&credentials),
        firefox::find,
    )
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::oauth::ApplicationCredentials;
    use super::{Token, resolve_initial_token};

    fn application_token(value: &str) -> Token {
        serde_json::from_value(serde_json::json!({
            "access_token": value,
            "refresh_token": "refresh",
            "obtained_at": 1
        }))
        .unwrap()
    }

    fn credentials() -> ApplicationCredentials {
        ApplicationCredentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
        }
    }

    #[test]
    fn saved_token_has_first_precedence() {
        let token = resolve_initial_token(
            Some(application_token("saved")),
            Some(credentials()),
            |_| -> Result<Token> { panic!("must not open OAuth browser") },
            || panic!("must not inspect Firefox"),
        )
        .unwrap();

        assert_eq!(token.access_token, "saved");
        assert!(token.is_refreshable());
    }

    #[test]
    fn saved_token_without_application_credentials_does_not_start_refresh() {
        let token = resolve_initial_token(
            Some(application_token("saved")),
            None,
            |_| -> Result<Token> { panic!("must not open OAuth browser") },
            || panic!("must not inspect Firefox"),
        )
        .unwrap();

        assert!(!token.is_refreshable());
    }

    #[test]
    fn complete_application_credentials_precede_firefox() {
        let token = resolve_initial_token(
            None,
            Some(credentials()),
            |_| Ok(application_token("application")),
            || panic!("must not inspect Firefox"),
        )
        .unwrap();

        assert_eq!(token.access_token, "application");
    }

    #[test]
    fn absent_application_credentials_fall_back_to_firefox() {
        let token = resolve_initial_token(
            None,
            None,
            |_| -> Result<Token> { panic!("must not open OAuth browser") },
            || Some(Token::from_firefox("browser".into(), "profile".into())),
        )
        .unwrap();

        assert_eq!(token.access_token, "browser");
        assert_eq!(token.source_label(), "Firefox (profile)");
    }

    #[test]
    fn missing_all_credentials_returns_actionable_error() {
        let error = resolve_initial_token(
            None,
            None,
            |_| -> Result<Token> { panic!("must not open OAuth browser") },
            || None,
        )
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("SOUNDCLOUD_CLIENT_ID"));
        assert!(message.contains("Firefox"));
    }
}
