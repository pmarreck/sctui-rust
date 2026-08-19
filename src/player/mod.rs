mod commands;
mod controller;
mod stream;
mod worker;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackOutcome {
    Started { attempt_id: u64, track_urn: String },
    Failed { attempt_id: u64, track_urn: String, message: String },
}

#[allow(unused_imports)]
pub use commands::PlayerCommand;
pub use controller::Player;
