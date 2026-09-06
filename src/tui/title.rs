use std::io::{self, IsTerminal, Write};

const STOPPED_TITLE: &str = "sctui";
const PLAYING_TITLE: &str = "🔊 sctui";

#[derive(Default)]
pub(crate) struct PlaybackTitleState {
    last_playing: Option<bool>,
}

impl PlaybackTitleState {
    /// Returns a new terminal title only when active playback changes state.
    pub(crate) fn next_title(&mut self, is_playing: bool) -> Option<&'static str> {
        if self.last_playing == Some(is_playing) {
            return None;
        }
        self.last_playing = Some(is_playing);
        Some(if is_playing {
            PLAYING_TITLE
        } else {
            STOPPED_TITLE
        })
    }
}

fn set_terminal_title(title: &str) {
    let mut stdout = io::stdout();
    if stdout.is_terminal() {
        let sequence = osc_title_sequence(title);
        let _ = stdout
            .write_all(sequence.as_bytes())
            .and_then(|()| stdout.flush());
    }
}

fn osc_title_sequence(title: &str) -> String {
    format!("\u{1b}]2;{title}\u{7}")
}

pub(crate) fn update_terminal_title(state: &mut PlaybackTitleState, is_playing: bool) {
    if let Some(title) = state.next_title(is_playing) {
        set_terminal_title(title);
    }
}

pub(crate) fn clear_playback_title() {
    set_terminal_title(STOPPED_TITLE);
}

#[cfg(test)]
mod tests {
    use super::{osc_title_sequence, PlaybackTitleState};

    #[test]
    fn title_sequence_matches_the_cross_terminal_osc_two_form() {
        assert_eq!(osc_title_sequence("🔊 sctui"), "\u{1b}]2;🔊 sctui\u{7}");
    }

    #[test]
    fn title_updates_only_on_playback_transitions_and_removes_the_speaker_on_stop() {
        let mut state = PlaybackTitleState::default();
        let observed = [false, false, true, true, false]
            .into_iter()
            .map(|playing| state.next_title(playing))
            .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![Some("sctui"), None, Some("🔊 sctui"), None, Some("sctui"),]
        );
    }
}
