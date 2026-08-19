use std::time::Duration;

use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::player::Player;
use crate::tui::layout::{
    CollectionTarget, TrackTarget, equal_tab_at_x, library_collection_region,
    library_track_regions, now_playing_progress_area, root_regions, search_track_regions,
    seek_position_ms, tab_at_x, table_row_at, visible_table_rows,
};
use crate::tui::logic::filtering::is_filter_active;
use crate::tui::logic::state::{AppData, AppState, FollowingTracksFocus};

use super::playback;

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

fn scroll_step(kind: MouseEventKind, modifiers: KeyModifiers) -> Option<isize> {
    let distance = if modifiers.contains(KeyModifiers::SHIFT) {
        5
    } else {
        1
    };
    match kind {
        MouseEventKind::ScrollUp => Some(-distance),
        MouseEventKind::ScrollDown => Some(distance),
        _ => None,
    }
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    if delta >= 0 {
        current.saturating_add(delta as usize).min(len - 1)
    } else {
        current.saturating_sub(delta.unsigned_abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickKind {
    Single,
    Double,
}

#[derive(Default)]
pub(crate) struct ClickTracker {
    last: Option<(TrackTarget, usize, Duration)>,
}

impl ClickTracker {
    pub(crate) fn register(&mut self, target: TrackTarget, row: usize, now: Duration) -> ClickKind {
        let double = self
            .last
            .is_some_and(|(prior_target, prior_row, prior_time)| {
                prior_target == target
                    && prior_row == row
                    && now.saturating_sub(prior_time) <= DOUBLE_CLICK_WINDOW
            });
        if double {
            self.last = None;
            ClickKind::Double
        } else {
            self.last = Some((target, row, now));
            ClickKind::Single
        }
    }
}

fn track_offset_and_len(target: TrackTarget, state: &AppState, data: &AppData) -> (usize, usize) {
    match target {
        TrackTarget::Likes => (
            data.likes_state.offset(),
            if is_filter_active(state) {
                state.search_matches.len()
            } else {
                data.likes.len()
            },
        ),
        TrackTarget::Playlist => (
            data.playlist_tracks_state.offset(),
            if is_filter_active(state) {
                state.search_matches.len()
            } else {
                data.playlist_tracks.len()
            },
        ),
        TrackTarget::Album => (data.album_tracks_state.offset(), data.album_tracks.len()),
        TrackTarget::FollowingPublished => (
            data.following_tracks_state.offset(),
            data.following_tracks.len(),
        ),
        TrackTarget::FollowingLikes => (
            data.following_likes_state.offset(),
            data.following_likes_tracks.len(),
        ),
        TrackTarget::SearchTracks => (data.search_tracks_state.offset(), data.search_tracks.len()),
        TrackTarget::SearchPlaylist => (
            data.search_playlist_tracks_state.offset(),
            data.search_playlist_tracks.len(),
        ),
        TrackTarget::SearchAlbum => (
            data.search_album_tracks_state.offset(),
            data.search_album_tracks.len(),
        ),
        TrackTarget::SearchPersonPublished => (
            data.search_people_tracks_state.offset(),
            data.search_people_tracks.len(),
        ),
        TrackTarget::SearchPersonLikes => (
            data.search_people_likes_state.offset(),
            data.search_people_likes_tracks.len(),
        ),
    }
}

fn select_track(target: TrackTarget, row: usize, state: &mut AppState, data: &mut AppData) {
    if matches!(
        target,
        TrackTarget::SearchTracks
            | TrackTarget::SearchPlaylist
            | TrackTarget::SearchAlbum
            | TrackTarget::SearchPersonPublished
            | TrackTarget::SearchPersonLikes
    ) {
        state.search_input_focused = false;
    }
    match target {
        TrackTarget::Likes | TrackTarget::SearchTracks => {
            state.selected_row = row;
            if target == TrackTarget::Likes {
                data.likes_state.select(Some(row));
            } else {
                data.search_tracks_state.select(Some(row));
            }
        }
        TrackTarget::Playlist => {
            state.selected_playlist_track_row = row;
            data.playlist_tracks_state.select(Some(row));
        }
        TrackTarget::Album => {
            state.selected_album_track_row = row;
            data.album_tracks_state.select(Some(row));
        }
        TrackTarget::FollowingPublished => {
            state.selected_following_track_row = row;
            state.following_tracks_focus = FollowingTracksFocus::Published;
            data.following_tracks_state.select(Some(row));
        }
        TrackTarget::FollowingLikes => {
            state.selected_following_like_row = row;
            state.following_tracks_focus = FollowingTracksFocus::Likes;
            data.following_likes_state.select(Some(row));
        }
        TrackTarget::SearchPlaylist => {
            state.search_selected_playlist_track_row = row;
            data.search_playlist_tracks_state.select(Some(row));
        }
        TrackTarget::SearchAlbum => {
            state.search_selected_album_track_row = row;
            data.search_album_tracks_state.select(Some(row));
        }
        TrackTarget::SearchPersonPublished => {
            state.search_selected_person_track_row = row;
            state.search_people_tracks_focus = FollowingTracksFocus::Published;
            data.search_people_tracks_state.select(Some(row));
        }
        TrackTarget::SearchPersonLikes => {
            state.search_selected_person_like_row = row;
            state.search_people_tracks_focus = FollowingTracksFocus::Likes;
            data.search_people_likes_state.select(Some(row));
        }
    }
}

fn selected_track_row(target: TrackTarget, state: &AppState) -> usize {
    match target {
        TrackTarget::Likes | TrackTarget::SearchTracks => state.selected_row,
        TrackTarget::Playlist => state.selected_playlist_track_row,
        TrackTarget::Album => state.selected_album_track_row,
        TrackTarget::FollowingPublished => state.selected_following_track_row,
        TrackTarget::FollowingLikes => state.selected_following_like_row,
        TrackTarget::SearchPlaylist => state.search_selected_playlist_track_row,
        TrackTarget::SearchAlbum => state.search_selected_album_track_row,
        TrackTarget::SearchPersonPublished => state.search_selected_person_track_row,
        TrackTarget::SearchPersonLikes => state.search_selected_person_like_row,
    }
}

pub(crate) fn active_track_target(state: &AppState) -> Option<TrackTarget> {
    match (state.selected_tab, state.selected_subtab, state.selected_searchfilter) {
        (0, 0, _) => Some(TrackTarget::Likes),
        (0, 1, _) => Some(TrackTarget::Playlist),
        (0, 2, _) => Some(TrackTarget::Album),
        (0, 3, _) if state.following_tracks_focus == FollowingTracksFocus::Likes => {
            Some(TrackTarget::FollowingLikes)
        }
        (0, 3, _) => Some(TrackTarget::FollowingPublished),
        (1, _, 0) => Some(TrackTarget::SearchTracks),
        (1, _, 1) => Some(TrackTarget::SearchAlbum),
        (1, _, 2) => Some(TrackTarget::SearchPlaylist),
        (1, _, 3) if state.search_people_tracks_focus == FollowingTracksFocus::Likes => {
            Some(TrackTarget::SearchPersonLikes)
        }
        (1, _, 3) => Some(TrackTarget::SearchPersonPublished),
        _ => None,
    }
}

pub(crate) fn active_track_page_rows(area: Rect, state: &AppState) -> usize {
    let target = match active_track_target(state) {
        Some(target) => target,
        None => return 0,
    };
    let root = root_regions(area);
    let regions = if state.selected_tab == 0 {
        library_track_regions(
            root.content,
            state.selected_subtab,
            state.search_popup_visible,
        )
    } else {
        search_track_regions(root.content, state.selected_searchfilter)
    };
    regions
        .into_iter()
        .find_map(|(candidate, region)| (candidate == target).then_some(visible_table_rows(region)))
        .unwrap_or(0)
}

pub(crate) fn move_track_selection(
    target: TrackTarget,
    delta: isize,
    state: &mut AppState,
    data: &mut AppData,
) {
    let (_, len) = track_offset_and_len(target, state, data);
    let row = move_index(selected_track_row(target, state), len, delta);
    select_track(target, row, state, data);
}

fn collection_offset_and_len(
    target: CollectionTarget,
    data: &AppData,
) -> (usize, usize) {
    match target {
        CollectionTarget::Playlist => (data.playlists_state.offset(), data.playlists.len()),
        CollectionTarget::Album => (data.albums_state.offset(), data.albums.len()),
        CollectionTarget::Following => (data.following_state.offset(), data.following.len()),
    }
}

fn select_collection(
    target: CollectionTarget,
    row: usize,
    state: &mut AppState,
    data: &mut AppData,
) {
    state.selected_row = row;
    match target {
        CollectionTarget::Playlist => {
            state.selected_playlist_row = row;
            data.playlists_state.select(Some(row));
        }
        CollectionTarget::Album => {
            state.selected_album_row = row;
            data.albums_state.select(Some(row));
        }
        CollectionTarget::Following => data.following_state.select(Some(row)),
    }
}

fn select_library_subtab(index: usize, state: &mut AppState, data: &mut AppData) {
    if state.selected_subtab == 1 {
        state.selected_playlist_row = state.selected_row;
    } else if state.selected_subtab == 2 {
        state.selected_album_row = state.selected_row;
    }
    state.selected_subtab = index;
    state.selected_row = match index {
        1 => state.selected_playlist_row,
        2 => state.selected_album_row,
        _ => 0,
    };
    match index {
        0 => data.likes_state.select(Some(state.selected_row)),
        1 => data.playlists_state.select(Some(state.selected_row)),
        2 => data.albums_state.select(Some(state.selected_row)),
        3 => data.following_state.select(Some(state.selected_row)),
        _ => {}
    }
}

fn select_search_filter(index: usize, state: &mut AppState, data: &mut AppData) {
    state.selected_searchfilter = index;
    state.selected_row = 0;
    state.search_selected_playlist_track_row = 0;
    state.search_selected_album_track_row = 0;
    state.search_selected_person_track_row = 0;
    state.search_selected_person_like_row = 0;
    state.search_people_tracks_focus = FollowingTracksFocus::Published;
    state.search_needs_fetch = !state.search_input_focused;
    data.search_tracks_state.select(Some(0));
    data.search_albums_state.select(Some(0));
    data.search_playlists_state.select(Some(0));
    data.search_people_state.select(Some(0));
}

pub(crate) fn handle_mouse_event(
    event: MouseEvent,
    area: Rect,
    now: Duration,
    clicks: &mut ClickTracker,
    state: &mut AppState,
    data: &mut AppData,
    player: &Player,
) {
    if state.visualizer_mode
        || state.queue_visible
        || state.help_visible
        || state.quit_confirm_visible
        || player.is_seeking()
    {
        return;
    }

    let root = root_regions(area);
    if let Some(delta) = scroll_step(event.kind, event.modifiers) {
        if event.row >= root.content.y && event.row < root.content.bottom() {
            if let Some(target) = active_track_target(state) {
                move_track_selection(target, delta, state, data);
            }
        }
        return;
    }

    if event.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }

    if event.row == root.tabs.y.saturating_add(1) {
        if let Some(tab) = tab_at_x(root.tabs, &["Library", "Search", "Feed"], event.column) {
            state.selected_tab = tab;
            state.selected_row = 0;
            state.search_input_focused = tab == 1;
            return;
        }
    }

    if state.selected_tab == 0 && event.row == root.content.y.saturating_add(1) {
        let subtab_area = Rect::new(root.content.x, root.content.y, root.content.width, 3);
        if let Some(tab) = tab_at_x(
            subtab_area,
            &["Likes", "Playlists", "Albums", "Following"],
            event.column,
        ) {
            select_library_subtab(tab, state, data);
            return;
        }
    }

    if state.selected_tab == 1 {
        let filter_area = Rect::new(
            root.content.x,
            root.content.bottom().saturating_sub(3),
            root.content.width,
            3,
        );
        if event.row == filter_area.y.saturating_add(1) {
            if let Some(tab) = equal_tab_at_x(filter_area, 4, event.column) {
                select_search_filter(tab, state, data);
                return;
            }
        }
    }

    let current_track = player.current_track();
    let progress_area = now_playing_progress_area(root.now_playing);
    if event.row == progress_area.y && !current_track.track_urn.is_empty() {
        if let Some(position) =
            seek_position_ms(progress_area, event.column, current_track.duration_ms)
        {
            player.seek(position);
            return;
        }
    }

    if state.selected_tab == 0 {
        if let Some((target, collection_area)) = library_collection_region(
            root.content,
            state.selected_subtab,
            state.search_popup_visible,
        ) {
            if event.column >= collection_area.x && event.column < collection_area.right() {
                let (offset, len) = collection_offset_and_len(target, data);
                if let Some(row) = table_row_at(collection_area, event.row, offset, len) {
                    select_collection(target, row, state, data);
                    return;
                }
            }
        }
    }

    let track_regions = match state.selected_tab {
        0 => library_track_regions(
            root.content,
            state.selected_subtab,
            state.search_popup_visible,
        ),
        1 => search_track_regions(root.content, state.selected_searchfilter),
        _ => Vec::new(),
    };
    for (target, track_area) in track_regions {
        if event.column < track_area.x || event.column >= track_area.right() {
            continue;
        }
        let (offset, len) = track_offset_and_len(target, state, data);
        let Some(row) = table_row_at(track_area, event.row, offset, len) else {
            continue;
        };
        select_track(target, row, state, data);
        if clicks.register(target, row, now) == ClickKind::Double {
            playback::handle_enter(state, data, player);
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::crossterm::event::{KeyModifiers, MouseEventKind};
    use ratatui::layout::Rect;

    use crate::tui::layout::TrackTarget;
    use crate::tui::logic::state::AppState;

    use super::{
        ClickKind, ClickTracker, active_track_page_rows, move_index, scroll_step,
    };

    #[test]
    fn double_click_requires_same_track_within_the_injected_deadline() {
        let mut clicks = ClickTracker::default();

        assert_eq!(
            clicks.register(TrackTarget::Likes, 3, Duration::from_millis(1_000)),
            ClickKind::Single
        );
        assert_eq!(
            clicks.register(TrackTarget::Likes, 4, Duration::from_millis(1_100)),
            ClickKind::Single
        );
        assert_eq!(
            clicks.register(TrackTarget::Likes, 4, Duration::from_millis(1_499)),
            ClickKind::Double
        );
        assert_eq!(
            clicks.register(TrackTarget::Likes, 4, Duration::from_millis(1_900)),
            ClickKind::Single
        );
    }

    #[test]
    fn wheel_steps_classify_direction_and_shift_modifier() {
        let cases = [
            (MouseEventKind::ScrollUp, KeyModifiers::NONE, -1),
            (MouseEventKind::ScrollDown, KeyModifiers::NONE, 1),
            (MouseEventKind::ScrollUp, KeyModifiers::SHIFT, -5),
            (MouseEventKind::ScrollDown, KeyModifiers::SHIFT, 5),
        ];

        let observed = cases
            .iter()
            .map(|(kind, modifiers, _)| scroll_step(*kind, *modifiers))
            .collect::<Vec<_>>();
        let expected = cases
            .iter()
            .map(|(_, _, expected)| Some(*expected))
            .collect::<Vec<_>>();

        assert_eq!(observed, expected);
        assert_eq!(scroll_step(MouseEventKind::Moved, KeyModifiers::SHIFT), None);
    }

    #[test]
    fn selection_movement_clamps_over_complete_boundary_set() {
        let cases = [
            (0, 0, 1, 0),
            (0, 10, -1, 0),
            (0, 10, 1, 1),
            (4, 10, 5, 9),
            (9, 10, 5, 9),
            (4, 10, -5, 0),
        ];

        let observed = cases
            .iter()
            .map(|(current, len, delta, _)| move_index(*current, *len, *delta))
            .collect::<Vec<_>>();
        let expected = cases
            .iter()
            .map(|(_, _, _, expected)| *expected)
            .collect::<Vec<_>>();

        assert_eq!(observed, expected);
    }

    #[test]
    fn page_size_comes_from_the_active_rendered_track_pane() {
        let area = Rect::new(0, 0, 120, 40);
        let mut state = AppState::new();

        assert_eq!(active_track_page_rows(area, &state), 23);
        state.selected_subtab = 1;
        assert_eq!(active_track_page_rows(area, &state), 23);
        state.selected_tab = 1;
        state.selected_searchfilter = 0;
        assert_eq!(active_track_page_rows(area, &state), 20);
        assert_eq!(active_track_page_rows(Rect::new(0, 0, 120, 10), &state), 0);
    }
}
