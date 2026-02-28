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
        AppEvent::PlaylistsLoaded(pl) => {
            state.playlists = pl;
            state.loaded_playlists = true;
            update_sidebar_selection(state);
            check_ready(state);
        }
        AppEvent::LikedTracksLoaded(tracks) => {
            state.liked_tracks = tracks;
            state.loaded_liked = true;
            if matches!(state.explorer_stack.last(), Some(ExplorerNode::LikedTracks)) {
                state.explorer_items = state.liked_tracks.clone();
                state.explorer_fetch_pending = false;
            }
            check_ready(state);
        }
        AppEvent::ExplorerTracksLoaded(tracks) => {
            state.explorer_items = tracks;
            state.explorer_selected_index = 0;
            state.explorer_fetch_pending = false;
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

        // ── Navigation ────────────────────────────────────────────────────────
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
            // If already in explorer, Enter is handled by main.rs to play the track
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

        // ── Playback ──────────────────────────────────────────────────────────
        AppEvent::PlayTrack {
            track: _,
            context_uri,
        } => {
            state.playing_context_uri = context_uri;
        }
        AppEvent::DevicesUpdated(devs) => {
            state.devices = devs;
        }
        AppEvent::TogglePause => {
            // HTTP call fired by main.rs; optimistically flip is_playing
            if let Some(p) = &mut state.playback {
                p.is_playing = !p.is_playing;
            }
        }
        AppEvent::PlaybackStateUpdated(ps) => {
            state.playback = ps;
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn move_cursor(state: &mut AppState, delta: isize) {
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
