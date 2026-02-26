#[derive(Clone)]
pub enum ExplorerNode {
    PlaylistTracks(String),
    ArtistAlbums(String),
    LikedTracks,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    Normal,
    AwaitingG,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Explorer,
}

pub struct NavigationState {
    pub selected_index: usize,
}

pub struct AppState {
    pub should_quit: bool,

    pub navigation: NavigationState,
    pub explorer_stack: Vec<ExplorerNode>,
    pub explorer_selected_index: usize,

    pub key_mode: KeyMode,
    pub focus: Focus,

    pub pending_count: Option<usize>,

    // ─────────────────────────────────────────────
    // Animation State
    // ─────────────────────────────────────────────
    pub playback_progress: f64, // 0.0 → 1.0
    pub visualizer_phase: usize,
}
