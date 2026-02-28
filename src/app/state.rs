use crate::services::spotify::{PlaylistSummary, TrackSummary};
use crate::ui::cover::CoverArt;
use std::collections::HashMap;

#[derive(Clone)]
pub enum ExplorerNode {
    PlaylistTracks(String, String, bool), // id, name, is_owner
    LikedTracks,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppStatus {
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyMode {
    Normal,
    AwaitingG,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Sidebar,
    Explorer,
}

pub struct NavigationState {
    pub selected_index: usize,
}

pub struct AppState {
    pub status: AppStatus,
    pub should_quit: bool,

    pub loaded_user: bool,
    pub loaded_playlists: bool,
    pub loaded_liked: bool,
    pub explorer_fetch_pending: bool,

    pub user_name: Option<String>,
    pub playlists: Vec<PlaylistSummary>,
    pub liked_tracks: Vec<TrackSummary>,
    pub explorer_items: Vec<TrackSummary>,

    /// Small covers: keyed by image_url → sidebar-sized art (8×4 cells)
    pub cover_cache_small: HashMap<String, CoverArt>,
    /// Large covers: keyed by image_url → detail panel art (16×16 cells)
    pub cover_cache_large: HashMap<String, CoverArt>,

    pub navigation: NavigationState,
    pub explorer_stack: Vec<ExplorerNode>,
    pub explorer_selected_index: usize,
    pub key_mode: KeyMode,
    pub focus: Focus,
    pub pending_count: Option<usize>,

    pub error_message: Option<String>,
    pub playback_progress: f64,
    pub visualizer_phase: usize,
}
