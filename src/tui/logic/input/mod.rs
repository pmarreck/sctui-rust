use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::player::Player;

use crate::tui::logic::state::{AppData, AppState};

mod commands;
mod helpers;
mod mouse;
mod movement;
mod navigation;
mod playback;
mod queue;
mod quit;
mod search;

pub(crate) use mouse::{ClickTracker, handle_mouse_event};

pub enum InputOutcome {
    Continue,
    Quit,
}

fn is_immediate_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q' | 'Q'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

pub fn handle_key_event(
    key: KeyEvent,
    area: Rect,
    state: &mut AppState,
    data: &mut AppData,
    player: &Player,
) -> InputOutcome {
    if is_immediate_quit(key) {
        return InputOutcome::Quit;
    }

    if state.quit_confirm_visible {
        return quit::handle_quit_confirm(key, state);
    }

    if state.search_popup_visible {
        if let Some(outcome) = search::handle_search_input(key, state, data) {
            return outcome;
        }
    }

    if state.visualizer_mode && key.code == KeyCode::Tab {
        state.visualizer_view = state.visualizer_view.next();
        return InputOutcome::Continue;
    }

    if player.is_seeking() {
        return InputOutcome::Continue;
    }

    match key.code {
        KeyCode::Esc => {
            state.quit_confirm_visible = true;
            state.quit_confirm_selected = 1;
            InputOutcome::Continue
        }
        KeyCode::Tab => navigation::handle_tab_switch(state),
        KeyCode::Right => navigation::handle_right_key(key, state, data, player),
        KeyCode::Left => navigation::handle_left_key(key, state, data, player),
        KeyCode::Down => movement::handle_down_key(key, state, data),
        KeyCode::Up => movement::handle_up_key(key, state, data),
        KeyCode::PageDown => {
            if let Some(target) = mouse::active_track_target(state) {
                let distance = mouse::active_track_page_rows(area, state);
                if distance > 0 {
                    mouse::move_track_selection(target, distance as isize, state, data);
                }
            }
            InputOutcome::Continue
        }
        KeyCode::PageUp => {
            if let Some(target) = mouse::active_track_target(state) {
                let distance = mouse::active_track_page_rows(area, state);
                if distance > 0 {
                    mouse::move_track_selection(target, -(distance as isize), state, data);
                }
            }
            InputOutcome::Continue
        }
        KeyCode::Char(c) => commands::handle_char(key, c, state, data, player),
        KeyCode::Backspace => commands::handle_backspace(state),
        KeyCode::Enter => {
            if commands::handle_search_submit(state) {
                InputOutcome::Continue
            } else {
                playback::handle_enter(state, data, player)
            }
        }
        _ => InputOutcome::Continue,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::is_immediate_quit;

    #[test]
    fn control_q_exits_before_any_focused_input_handles_the_character() {
        assert!(is_immediate_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
        )));
        assert!(is_immediate_quit(KeyEvent::new(
            KeyCode::Char('Q'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )));
        assert!(!is_immediate_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )));
    }
}
