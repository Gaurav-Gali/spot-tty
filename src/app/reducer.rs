use super::{
    events::AppEvent,
    state::{AppState, AppStatus, ExplorerNode, Focus, KeyMode},
};
use std::time::Instant;

pub fn reduce(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::Quit => state.should_quit = true,

        AppEvent::UserLoaded(name) => {
            state.user_name = Some(name);
            state.loaded_user = true;
            check_ready(state);
        }
        AppEvent::UserProfileLoaded(profile) => {
            state.user_profile = Some(profile);
            recompute_stats(state);
        }
        AppEvent::PlaylistsLoaded(pl) => {
            state.playlists = pl;
            state.loaded_playlists = true;
            update_sidebar_selection(state);
            check_ready(state);
            recompute_stats(state);
        }
        AppEvent::LikedTracksLoaded(tracks) => {
            state.merge_tracks(&tracks.clone());
            state.liked_tracks = tracks;
            state.loaded_liked = true;
            recompute_stats(state);
            if matches!(state.explorer_stack.last(), Some(ExplorerNode::LikedTracks)) {
                state.explorer_items = state.liked_tracks.clone();
                state.explorer_fetch_pending = false;
            }
            check_ready(state);
        }
        AppEvent::ExplorerTracksLoaded(tracks) => {
            state.merge_tracks(&tracks.clone());
            state.explorer_items = tracks;
            state.explorer_selected_index = 0;
            state.explorer_fetch_pending = false;
            recompute_stats(state);
        }
        AppEvent::CoverLoaded(url, img) => {
            state.cover_fetching.remove(&url);
            state.cover_cache.insert(url, img);
        }
        AppEvent::LoadError(msg) => {
            tracing::error!("{msg}");
            state.error_message = Some(msg);
            check_ready(state);
        }

        AppEvent::MoveDown(n) => {
            move_cursor(state, n as isize);
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::MoveUp(n) => {
            move_cursor(state, -(n as isize));
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::GoTop => {
            set_cursor(state, 0);
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::GoBottom => {
            let m = max_index(state);
            set_cursor(state, m);
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::GoMiddle => {
            let m = max_index(state);
            set_cursor(state, m / 2);
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::Enter => {
            if state.focus == Focus::Sidebar {
                state.focus = Focus::Explorer;
            }
        }
        AppEvent::Back => {
            state.focus = Focus::Sidebar;
        }
        AppEvent::JumpToPlaylists => {
            state.navigation.selected_index = 0;
            update_sidebar_selection(state);
            state.key_mode = KeyMode::Normal;
            state.last_nav_move = Some(Instant::now());
        }
        AppEvent::JumpToLiked => {
            state.navigation.selected_index = state.playlists.len();
            update_sidebar_selection(state);
            state.key_mode = KeyMode::Normal;
            state.last_nav_move = Some(Instant::now());
        }

        AppEvent::PlayTrack { context_uri, .. } => {
            state.playing_context_uri = context_uri;
        }
        AppEvent::TogglePause => {
            if let Some(p) = &mut state.playback {
                p.is_playing = !p.is_playing;
            }
        }
        AppEvent::SkipNext => {
            if let Some(p) = &mut state.playback {
                p.progress_ms = 0;
            }
        }
        AppEvent::SkipPrev => {
            if let Some(p) = &mut state.playback {
                p.progress_ms = 0;
            }
        }
        AppEvent::PlaybackStateUpdated(ps) => {
            state.playback = ps;
        }
        AppEvent::DevicesUpdated(devs) => {
            state.devices = devs;
        }

        AppEvent::OpenSearch => {
            state.key_mode = KeyMode::Search;
            state.search.query.clear();
            state.search.is_searching = false;
            let tracks = state.all_tracks.clone();
            state.search.update_local(&tracks);
        }
        AppEvent::CloseSearch => {
            state.key_mode = KeyMode::Normal;
        }
        AppEvent::SearchQueryChanged(q) => {
            state.search.query = q;
            let tracks = state.all_tracks.clone();
            state.search.update_local(&tracks);
            if !state.search.query.is_empty() {
                state.search.is_searching = true;
            }
        }
        AppEvent::SearchCatalogResults(results) => {
            state.search.merge_catalog(results);
        }

        AppEvent::OpenTrackMenu => {
            if let Some(track) = state
                .explorer_items
                .get(state.explorer_selected_index)
                .cloned()
            {
                state.track_menu = crate::ui::trackmenu::TrackMenuState::open(track);
                state.key_mode = KeyMode::TrackMenu;
            }
        }
        AppEvent::CloseTrackMenu => {
            state.key_mode = KeyMode::Normal;
            state.track_menu = Default::default();
        }
        AppEvent::TrackMenuQueryChanged(q) => {
            state.track_menu.query = q;
            state.track_menu.rebuild_actions();
        }
        AppEvent::TrackMenuConfirm => {}

        AppEvent::OpenProfile => {
            state.key_mode = crate::app::state::KeyMode::Profile;
        }
        AppEvent::CloseProfile => {
            state.key_mode = crate::app::state::KeyMode::Normal;
        }
        AppEvent::ProfileSectionNext => {
            state.profile.next_section();
        }
        AppEvent::ProfileSectionPrev => {
            state.profile.prev_section();
        }
        AppEvent::ProfileLogout => {
            let path = crate::services::auth::token_cache_path();
            let _ = std::fs::remove_file(&path);
            state.should_quit = true;
        }

        AppEvent::Toast(msg) => {
            state.show_toast(msg);
        }
    }
}

fn move_cursor(state: &mut AppState, delta: isize) {
    match state.key_mode {
        KeyMode::Search => {
            let max = state.search.results.len().saturating_sub(1);
            state.search.selected =
                (state.search.selected as isize + delta).clamp(0, max as isize) as usize;
            return;
        }
        KeyMode::TrackMenu => {
            let max = state.track_menu.actions.len().saturating_sub(1);
            state.track_menu.selected =
                (state.track_menu.selected as isize + delta).clamp(0, max as isize) as usize;
            return;
        }
        KeyMode::Profile => {
            if delta > 0 {
                state.profile.next_section();
            } else {
                state.profile.prev_section();
            }
            return;
        }
        _ => {}
    }
    let max = max_index(state) as isize;
    match state.focus {
        Focus::Sidebar => {
            let i = (state.navigation.selected_index as isize + delta).clamp(0, max) as usize;
            state.navigation.selected_index = i;
            update_sidebar_selection(state);
        }
        Focus::Explorer => {
            let i = (state.explorer_selected_index as isize + delta).clamp(0, max) as usize;
            state.explorer_selected_index = i;
        }
    }
}

fn set_cursor(state: &mut AppState, idx: usize) {
    match state.focus {
        Focus::Sidebar => {
            state.navigation.selected_index = idx.min(max_index(state));
            update_sidebar_selection(state);
        }
        Focus::Explorer => {
            state.explorer_selected_index = idx.min(max_index(state));
        }
    }
}

fn max_index(state: &AppState) -> usize {
    match state.focus {
        Focus::Sidebar => (state.playlists.len() + 1).saturating_sub(1),
        Focus::Explorer => state.explorer_items.len().saturating_sub(1),
    }
}

fn update_sidebar_selection(state: &mut AppState) {
    let idx = state.navigation.selected_index;
    let pl_len = state.playlists.len();
    let new_node = if idx < pl_len {
        let pl = &state.playlists[idx];
        ExplorerNode::PlaylistTracks(pl.id.clone(), pl.name.clone(), pl.owner)
    } else {
        ExplorerNode::LikedTracks
    };
    let changed = match state.explorer_stack.last() {
        None => true,
        Some(n) => !nodes_equal(n, &new_node),
    };
    state.explorer_stack.clear();
    state.explorer_stack.push(new_node);
    state.explorer_selected_index = 0;
    if changed {
        state.explorer_items.clear();
        state.explorer_fetch_pending = true;
        if matches!(state.explorer_stack.last(), Some(ExplorerNode::LikedTracks)) {
            state.explorer_items = state.liked_tracks.clone();
            state.explorer_fetch_pending = false;
        }
    }
}

fn nodes_equal(a: &ExplorerNode, b: &ExplorerNode) -> bool {
    match (a, b) {
        (ExplorerNode::PlaylistTracks(id1, ..), ExplorerNode::PlaylistTracks(id2, ..)) => {
            id1 == id2
        }
        (ExplorerNode::LikedTracks, ExplorerNode::LikedTracks) => true,
        _ => false,
    }
}

fn check_ready(state: &mut AppState) {
    if state.status == AppStatus::Loading
        && state.loaded_user
        && state.loaded_playlists
        && state.loaded_liked
    {
        state.status = AppStatus::Ready;
    }
}

fn recompute_stats(state: &mut AppState) {
    state.cached_stats = crate::services::spotify::compute_stats(
        state.user_profile.as_ref().unwrap_or(&Default::default()),
        &state.playlists,
        &state.liked_tracks,
    );
}
