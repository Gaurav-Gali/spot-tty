use super::events::AppEvent;
use super::reducer::reduce;
use super::state::{AppState, Focus, KeyMode, NavigationState};

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState {
                should_quit: false,

                navigation: NavigationState { selected_index: 0 },

                explorer_stack: vec![],
                explorer_selected_index: 0,

                key_mode: KeyMode::Normal,
                focus: Focus::Sidebar,

                pending_count: None,

                // Animation initial state
                playback_progress: 0.0,
                visualizer_phase: 0,
            },
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        reduce(&mut self.state, event);
    }
}
