use crate::services::spotify::{ArtistSummary, PlaylistSummary, TrackSummary};

// ─────────────────────────────────────────────────────────────────────────────
// Explorer node — now carries the playlist/artist ID for live fetching
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum ExplorerNode {
    /// (playlist_id, display_name)
    PlaylistTracks(String, String),
    /// (artist_id, display_name)
    ArtistAlbums(String, String),
    LikedTracks,
}

// ─────────────────────────────────────────────────────────────────────────────
// App-level loading state
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppStatus {
    /// Waiting for auth / initial data fetch
    Loading,
    /// All initial data loaded — normal navigation
    Ready,
    /// An unrecoverable error occurred
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

// ─────────────────────────────────────────────────────────────────────────────
// AppState
// ─────────────────────────────────────────────────────────────────────────────

pub struct AppState {
    pub status: AppStatus,
    pub should_quit: bool,

    // ── User ─────────────────────────────────────────────────────────────
    pub user_name: Option<String>,

    // ── Library data (populated after auth) ──────────────────────────────
    pub playlists: Vec<PlaylistSummary>,
    pub liked_tracks: Vec<TrackSummary>,
    pub artists: Vec<ArtistSummary>,

    // ── Explorer content (tracks / albums for the selected item) ─────────
    pub explorer_items: Vec<TrackSummary>, // tracks in the selected playlist/liked
    pub explorer_albums: Vec<String>,      // album names for selected artist

    // ── Navigation ───────────────────────────────────────────────────────
    pub navigation: NavigationState,
    pub explorer_stack: Vec<ExplorerNode>,
    pub explorer_selected_index: usize,
    pub key_mode: KeyMode,
    pub focus: Focus,
    pub pending_count: Option<usize>,

    // ── Error message ─────────────────────────────────────────────────────
    pub error_message: Option<String>,

    // ── Animation State ───────────────────────────────────────────────────
    pub playback_progress: f64,
    pub visualizer_phase: usize,
}
