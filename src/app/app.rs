use super::events::AppEvent;
use super::reducer::reduce;
use super::state::{AppState, AppStatus, Focus, KeyMode, NavigationState};
use std::collections::HashMap;

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState {
                status: AppStatus::Loading,
                should_quit: false,
                loaded_user: false,
                loaded_playlists: false,
                loaded_liked: false,
                explorer_fetch_pending: false,
                user_name: None,
                playlists: vec![],
                liked_tracks: vec![],
                explorer_items: vec![],
                cover_cache_small: HashMap::new(),
                cover_cache_large: HashMap::new(),
                navigation: NavigationState { selected_index: 0 },
                explorer_stack: vec![],
                explorer_selected_index: 0,
                key_mode: KeyMode::Normal,
                focus: Focus::Sidebar,
                pending_count: None,
                error_message: None,
                playback_progress: 0.3,
                visualizer_phase: 0,
            },
        }
    }
    pub fn handle_event(&mut self, event: AppEvent) {
        reduce(&mut self.state, event);
    }
}
