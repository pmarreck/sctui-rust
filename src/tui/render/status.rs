use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const SHORTCUTS: &str = "Ctrl-Q Quit │ Esc Quit menu │ Tab Main tab │ ←/→ Section │ ↑/↓ Select │ Enter Play │ Space Pause │ ⇧←/→ Previous/next │ Alt←/→ Seek ±10s │ Alt↑/↓ Move ±10 │ ⇧↑/↓ Secondary │ ⇧J/K Tertiary │ ⇧U/D Volume │ ⇧S Shuffle │ ⇧R Repeat │ ⇧A Add queue │ ⇧N Play next │ ⇧L Like/follow │ ⇧V Visualizer │ ⇧F Find │ ⇧Q Queue │ ⇧H Help │ PgUp/PgDn Page │ Wheel Select │ ⇧Wheel ±5 │ Search Enter Submit";
const MARQUEE_GAP: &str = "     ";

/// Produces a deterministic one-line window over the complete shortcut list.
/// The caller supplies the scroll offset so rendering never reads a clock.
pub(crate) fn shortcut_window(width: usize, scroll: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = SHORTCUTS.chars().collect();
    if chars.len() <= width {
        return SHORTCUTS.to_string();
    }
    let cycle: Vec<char> = format!("{SHORTCUTS}{MARQUEE_GAP}").chars().collect();
    (0..width)
        .map(|index| cycle[(scroll + index) % cycle.len()])
        .collect()
}

pub(crate) fn status_line(
    width: usize,
    playback_error: Option<&str>,
    scroll: usize,
) -> Line<'static> {
    match playback_error {
        Some(error) => Line::from(Span::styled(
            format!("Playback error: {error}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled(
            shortcut_window(width, scroll),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )),
    }
}

pub(crate) fn render_status(
    frame: &mut Frame,
    area: Rect,
    playback_error: Option<&str>,
    scroll: usize,
) {
    frame.render_widget(
        Paragraph::new(status_line(area.width.into(), playback_error, scroll)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier};

    use super::{shortcut_window, status_line};

    #[test]
    fn approved_status_views_are_stable_at_common_widths() {
        assert_eq!(
            shortcut_window(120, 0),
            "Ctrl-Q Quit │ Esc Quit menu │ Tab Main tab │ ←/→ Section │ ↑/↓ Select │ Enter Play │ Space Pause │ ⇧←/→ Previous/next │ "
        );
        assert_eq!(
            shortcut_window(80, 0),
            "Ctrl-Q Quit │ Esc Quit menu │ Tab Main tab │ ←/→ Section │ ↑/↓ Select │ Enter Pl"
        );
        assert_eq!(
            shortcut_window(120, 120),
            "Alt←/→ Seek ±10s │ Alt↑/↓ Move ±10 │ ⇧↑/↓ Secondary │ ⇧J/K Tertiary │ ⇧U/D Volume │ ⇧S Shuffle │ ⇧R Repeat │ ⇧A Add queu"
        );
    }

    #[test]
    fn shortcut_style_is_subdued_italic_and_errors_are_red() {
        let shortcuts = status_line(80, None, 0);
        assert_eq!(shortcuts.spans[0].style.fg, Some(Color::DarkGray));
        assert!(
            shortcuts.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );

        let error = status_line(80, Some("resolver returned 403"), 0);
        assert_eq!(
            error.spans[0].content,
            "Playback error: resolver returned 403"
        );
        assert_eq!(error.spans[0].style.fg, Some(Color::Red));
        assert!(error.spans[0].style.add_modifier.contains(Modifier::BOLD));
    }
}
