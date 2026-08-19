use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackTarget {
    Likes,
    Playlist,
    Album,
    FollowingPublished,
    FollowingLikes,
    SearchTracks,
    SearchPlaylist,
    SearchAlbum,
    SearchPersonPublished,
    SearchPersonLikes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootRegions {
    pub tabs: Rect,
    pub content: Rect,
    pub status: Rect,
    pub now_playing: Rect,
}

pub(crate) fn root_regions(area: Rect) -> RootRegions {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(7),
        ])
        .split(area);
    RootRegions {
        tabs: chunks[0],
        content: chunks[1],
        status: chunks[2],
        now_playing: chunks[3],
    }
}

pub(crate) fn library_track_regions(
    area: Rect,
    selected_subtab: usize,
    search_popup_visible: bool,
) -> Vec<(TrackTarget, Rect)> {
    let subchunks = if search_popup_visible {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area)
    };
    let table_area = subchunks[if search_popup_visible { 2 } else { 1 }];

    match selected_subtab {
        0 => vec![(TrackTarget::Likes, table_area)],
        1 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
                .split(table_area);
            vec![(TrackTarget::Playlist, columns[1])]
        }
        2 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(table_area);
            vec![(TrackTarget::Album, columns[1])]
        }
        3 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(40),
                    Constraint::Percentage(40),
                ])
                .split(table_area);
            vec![
                (TrackTarget::FollowingPublished, columns[1]),
                (TrackTarget::FollowingLikes, columns[2]),
            ]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn search_track_regions(
    area: Rect,
    selected_searchfilter: usize,
) -> Vec<(TrackTarget, Rect)> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);
    let table_area = chunks[1];
    match selected_searchfilter {
        0 => vec![(TrackTarget::SearchTracks, table_area)],
        1 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(table_area);
            vec![(TrackTarget::SearchAlbum, columns[1])]
        }
        2 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
                .split(table_area);
            vec![(TrackTarget::SearchPlaylist, columns[1])]
        }
        3 => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(40),
                    Constraint::Percentage(40),
                ])
                .split(table_area);
            vec![
                (TrackTarget::SearchPersonPublished, columns[1]),
                (TrackTarget::SearchPersonLikes, columns[2]),
            ]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn table_row_at(area: Rect, y: u16, offset: usize, len: usize) -> Option<usize> {
    let first_row = area.y.saturating_add(2);
    let last_exclusive = area.y.saturating_add(area.height).saturating_sub(1);
    if y < first_row || y >= last_exclusive {
        return None;
    }
    let row = offset.saturating_add(usize::from(y - first_row));
    (row < len).then_some(row)
}

pub(crate) fn seek_position_ms(area: Rect, x: u16, duration_ms: u64) -> Option<u64> {
    if area.width == 0 || x < area.x || x >= area.x.saturating_add(area.width) {
        return None;
    }
    if area.width == 1 || duration_ms == 0 {
        return Some(0);
    }
    let numerator = u64::from(x - area.x).saturating_mul(duration_ms);
    Some(numerator / u64::from(area.width - 1))
}

pub(crate) fn now_playing_progress_area(area: Rect) -> Rect {
    let outer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(area);
    let padding = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(1),
            Constraint::Percentage(98),
            Constraint::Percentage(1),
        ])
        .split(outer[1]);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(1),
            Constraint::Length(12),
            Constraint::Percentage(5),
            Constraint::Min(0),
            Constraint::Percentage(5),
            Constraint::Length(9),
            Constraint::Percentage(1),
        ])
        .split(padding[1]);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(20),
        ])
        .split(horizontal[3]);
    vertical[5]
}

pub(crate) fn tab_at_x(area: Rect, titles: &[&str], x: u16) -> Option<usize> {
    if area.width < 2 || x <= area.x || x >= area.x.saturating_add(area.width).saturating_sub(1) {
        return None;
    }
    let mut cursor = area.x.saturating_add(1);
    for (index, title) in titles.iter().enumerate() {
        let width = u16::try_from(title.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        if x >= cursor && x < cursor.saturating_add(width) {
            return Some(index);
        }
        cursor = cursor.saturating_add(width).saturating_add(1);
    }
    None
}

pub(crate) fn equal_tab_at_x(area: Rect, count: usize, x: u16) -> Option<usize> {
    if count == 0 || area.width < 2 || x <= area.x || x >= area.right().saturating_sub(1) {
        return None;
    }
    let relative = usize::from(x - area.x - 1);
    let inner_width = usize::from(area.width - 2);
    Some((relative.saturating_mul(count) / inner_width.max(1)).min(count - 1))
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{
        TrackTarget, equal_tab_at_x, library_track_regions, root_regions, seek_position_ms,
        tab_at_x, table_row_at,
    };

    #[test]
    fn root_layout_reserves_status_between_content_and_player() {
        let regions = root_regions(Rect::new(0, 0, 120, 40));

        assert_eq!(regions.tabs, Rect::new(0, 0, 120, 3));
        assert_eq!(regions.content, Rect::new(0, 3, 120, 29));
        assert_eq!(regions.status, Rect::new(0, 32, 120, 1));
        assert_eq!(regions.now_playing, Rect::new(0, 33, 120, 7));
    }

    #[test]
    fn playlist_track_hit_region_matches_the_rendered_right_pane() {
        let regions = library_track_regions(Rect::new(0, 3, 120, 29), 1, false);

        assert_eq!(
            regions,
            vec![(TrackTarget::Playlist, Rect::new(40, 6, 80, 26))]
        );
    }

    #[test]
    fn table_rows_exclude_border_and_header_and_include_scroll_offset() {
        let area = Rect::new(10, 6, 50, 8);

        assert_eq!(table_row_at(area, 7, 20, 100), None);
        assert_eq!(table_row_at(area, 8, 20, 100), Some(20));
        assert_eq!(table_row_at(area, 12, 20, 100), Some(24));
        assert_eq!(table_row_at(area, 13, 20, 100), None);
    }

    #[test]
    fn progress_clicks_map_left_middle_and_right_without_overflow() {
        let area = Rect::new(10, 30, 101, 1);

        assert_eq!(seek_position_ms(area, 10, 240_000), Some(0));
        assert_eq!(seek_position_ms(area, 60, 240_000), Some(120_000));
        assert_eq!(seek_position_ms(area, 110, 240_000), Some(240_000));
        assert_eq!(seek_position_ms(area, 111, 240_000), None);
        assert_eq!(seek_position_ms(area, 10, 0), Some(0));
    }

    #[test]
    fn tab_hits_follow_rendered_padding_titles_and_dividers() {
        let area = Rect::new(0, 0, 120, 3);

        assert_eq!(tab_at_x(area, &["Library", "Search", "Feed"], 1), Some(0));
        assert_eq!(tab_at_x(area, &["Library", "Search", "Feed"], 9), Some(0));
        assert_eq!(tab_at_x(area, &["Library", "Search", "Feed"], 10), None);
        assert_eq!(tab_at_x(area, &["Library", "Search", "Feed"], 11), Some(1));
        assert_eq!(tab_at_x(area, &["Library", "Search", "Feed"], 20), Some(2));
    }

    #[test]
    fn equal_width_filter_tabs_include_both_outer_quarters() {
        let area = Rect::new(0, 10, 102, 3);

        assert_eq!(equal_tab_at_x(area, 4, 1), Some(0));
        assert_eq!(equal_tab_at_x(area, 4, 25), Some(0));
        assert_eq!(equal_tab_at_x(area, 4, 26), Some(1));
        assert_eq!(equal_tab_at_x(area, 4, 100), Some(3));
    }
}
