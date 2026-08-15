use std::sync::{Arc, Mutex, mpsc};
mod api;
mod auth;
mod player;
mod tui;
use player::Player;

fn main() -> anyhow::Result<()> {
    let token = auth::initial_token()?;
    let refreshable = token.is_refreshable();

    let token = Arc::new(Mutex::new(token));

    let mut api = Arc::new(Mutex::new(api::API::init(Arc::clone(&token))));

    let player = Player::new(Arc::clone(&token));

    if refreshable {
        let (reauth_tx, reauth_rx) = mpsc::channel();
        auth::start_auto_refresh(Arc::clone(&token), reauth_tx);

        let token_clone = Arc::clone(&token);
        let api_clone = Arc::clone(&api);
        std::thread::spawn(move || {
            for _ in reauth_rx {
                match auth::authenticate() {
                    Ok(new_token) => {
                        *token_clone.lock().unwrap() = new_token;
                        *api_clone.lock().unwrap() = api::API::init(Arc::clone(&token_clone));
                    }
                    Err(_) => {
                        // Re-authentication will retry after the next refresh failure.
                    }
                }
            }
        });
    }

    tui::run(&mut api, player).map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}
