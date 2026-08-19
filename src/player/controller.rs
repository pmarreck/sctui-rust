use crate::api::Track;
use crate::auth::Token;
use rodio::Sink;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

use super::commands::PlayerCommand;
use super::PlaybackOutcome;
use super::worker::player_loop;

fn playback_restriction_message(track: &Track) -> Option<String> {
    track
        .playback_restriction
        .map(|restriction| restriction.message().to_string())
}

pub struct Player {
    tx: Sender<PlayerCommand>,
    is_playing_flag: Arc<AtomicBool>,
    is_seeking_flag: Arc<AtomicBool>,
    elapsed_time: Arc<Mutex<Duration>>,
    last_start: Arc<Mutex<Option<Instant>>>,
    current_track: Arc<Mutex<Option<Track>>>,
    sink: Arc<Mutex<Option<Sink>>>,
    wave_buffer: Arc<Mutex<VecDeque<f32>>>,
    last_error: Arc<Mutex<Option<String>>>,
    outcomes: Arc<Mutex<VecDeque<PlaybackOutcome>>>,
    next_attempt_id: AtomicU64,
    requested_attempt_id: AtomicU64,
}

impl Player {
    pub fn new(token: Arc<Mutex<Token>>) -> Self {
        let (tx, rx) = mpsc::channel();
        let is_playing_flag = Arc::new(AtomicBool::new(false));
        let is_seeking_flag = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(Mutex::new(None));
        let elapsed_time = Arc::new(Mutex::new(Duration::ZERO));
        let last_start = Arc::new(Mutex::new(None));
        let current_track = Arc::new(Mutex::new(None));
        let wave_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let last_error = Arc::new(Mutex::new(None));
        let outcomes = Arc::new(Mutex::new(VecDeque::new()));

        {
            let flag_clone = Arc::clone(&is_playing_flag);
            let sink_clone = Arc::clone(&sink);
            let token_clone = Arc::clone(&token);
            let elapsed_clone = Arc::clone(&elapsed_time);
            let last_start_clone = Arc::clone(&last_start);
            let track_clone = Arc::clone(&current_track);
            let seeking_clone = Arc::clone(&is_seeking_flag);
            let wave_buffer_clone = Arc::clone(&wave_buffer);
            let last_error_clone = Arc::clone(&last_error);
            let outcomes_clone = Arc::clone(&outcomes);

            thread::spawn(move || {
                player_loop(
                    rx,
                    token_clone,
                    flag_clone,
                    seeking_clone,
                    sink_clone,
                    elapsed_clone,
                    last_start_clone,
                    track_clone,
                    wave_buffer_clone,
                    last_error_clone,
                    outcomes_clone,
                );
            });
        }

        Self {
            tx,
            is_playing_flag,
            is_seeking_flag,
            elapsed_time,
            last_start,
            current_track,
            sink,
            wave_buffer,
            last_error,
            outcomes,
            next_attempt_id: AtomicU64::new(0),
            requested_attempt_id: AtomicU64::new(0),
        }
    }

    pub fn play(&self, track: Track) {
        let attempt_id = self.begin_attempt();
        if let Some(message) = playback_restriction_message(&track) {
            *self.last_error.lock().unwrap() = Some(message.clone());
            self.outcomes
                .lock()
                .unwrap()
                .push_back(PlaybackOutcome::Failed {
                    attempt_id,
                    track_urn: track.track_urn,
                    message,
                });
            return;
        }
        let _ = self.tx.send(PlayerCommand::Play(track, attempt_id));
    }

    pub fn pause(&self) {
        let _ = self.tx.send(PlayerCommand::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(PlayerCommand::Resume);
    }

    pub fn volume_up(&self) {
        let _ = self.tx.send(PlayerCommand::VolumeUp);
    }

    pub fn volume_down(&self) {
        let _ = self.tx.send(PlayerCommand::VolumeDown);
    }

    #[allow(dead_code)]
    pub fn next_song(&self) {
        let _ = self.tx.send(PlayerCommand::NextSong);
    }

    #[allow(dead_code)]
    pub fn prev_song(&self) {
        let _ = self.tx.send(PlayerCommand::PrevSong);
    }

    pub fn fast_forward(&self) {
        let _ = self.tx.send(PlayerCommand::FastForward);
    }

    pub fn rewind(&self) {
        let _ = self.tx.send(PlayerCommand::Rewind);
    }

    /// Restarts the current track at the requested absolute position.
    pub fn seek(&self, position_ms: u64) {
        if let Some(track) = self.current_track.lock().unwrap().clone() {
            let clamped = position_ms.min(track.duration_ms);
            let attempt_id = self.begin_attempt();
            let _ = self
                .tx
                .send(PlayerCommand::PlayFromPosition(track, clamped, attempt_id));
        }
    }

    pub fn playback_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    /// Returns each worker result once so the UI can react without polling stale errors.
    pub(crate) fn take_playback_outcome(&self) -> Option<PlaybackOutcome> {
        self.outcomes.lock().unwrap().pop_front()
    }

    /// Identifies the newest request, even when the same SoundCloud track was selected twice.
    pub(crate) fn requested_attempt_id(&self) -> u64 {
        self.requested_attempt_id.load(Ordering::SeqCst)
    }

    fn begin_attempt(&self) -> u64 {
        let attempt_id = self.next_attempt_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.requested_attempt_id.store(attempt_id, Ordering::SeqCst);
        attempt_id
    }

    pub fn preload_next(&self, track: Track) {
        let _ = self.tx.send(PlayerCommand::PreloadNext(track));
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing_flag.load(Ordering::SeqCst)
    }

    pub fn is_seeking(&self) -> bool {
        self.is_seeking_flag.load(Ordering::SeqCst)
    }

    pub fn elapsed(&self) -> u64 {
        let mut elapsed = *self.elapsed_time.lock().unwrap();
        if self.is_playing() {
            if let Some(start) = *self.last_start.lock().unwrap() {
                elapsed += start.elapsed();
            }
        }
        elapsed.as_millis().try_into().unwrap()
    }

    pub fn current_track(&self) -> Track {
        self.current_track
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Track {
                title: "No Track Playing - Press <ENTER> on Something to Play!".to_string(),
                artists: "N/A".to_string(),
                duration: "0:00".to_string(),
                duration_ms: 1,
                playback_count: "0".to_string(),
                artwork_url: "".to_string(),
                stream_url: "".to_string(),
                access: "playable".to_string(),
                playback_restriction: None,
                track_urn: "".to_string(),
            })
    }

    pub fn get_volume(&self) -> f32 {
        if let Some(ref s) = *self.sink.lock().unwrap() {
            s.volume()
        } else {
            1.0
        }
    }

    pub fn wave_buffer(&self) -> Arc<Mutex<VecDeque<f32>>> {
        Arc::clone(&self.wave_buffer)
    }
}

#[cfg(test)]
mod tests {
    use crate::api::{PlaybackRestriction, Track};

    use super::playback_restriction_message;

    #[test]
    fn playback_request_reports_service_policy_before_starting_the_worker() {
        let track = Track {
            title: "Restricted fixture".into(),
            artists: "Fixture".into(),
            duration: "03:00".into(),
            duration_ms: 180_000,
            playback_count: "1".into(),
            artwork_url: String::new(),
            stream_url: "https://resolver/dead-legacy".into(),
            access: String::new(),
            playback_restriction: Some(PlaybackRestriction::SoundCloudGoPlus),
            track_urn: "soundcloud:tracks:1".into(),
        };

        assert_eq!(
            playback_restriction_message(&track).as_deref(),
            Some(
                "SoundCloud Go+ track requires encrypted high-tier playback, which sctui does not support"
            )
        );
    }
}
