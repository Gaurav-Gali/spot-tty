use crate::services::spotify::{ArtistSummary, PlaylistSummary, TrackSummary};

pub enum AppEvent {
    // ── Lifecycle ─────────────────────────────────────────────────────────
    Quit,

    // ── Data loading results (sent from async tasks → main loop) ─────────
    UserLoaded(String), // display name
    PlaylistsLoaded(Vec<PlaylistSummary>),
    LikedTracksLoaded(Vec<TrackSummary>),
    ArtistsLoaded(Vec<ArtistSummary>),
    ExplorerTracksLoaded(Vec<TrackSummary>), // tracks for selected playlist / liked
    ExplorerAlbumsLoaded(Vec<String>),       // albums for selected artist
    LoadError(String),

    // ── Navigation ────────────────────────────────────────────────────────
    MoveDown(usize),
    MoveUp(usize),
    GoTop,
    GoBottom,
    GoMiddle,
    Enter,
    Back,
    JumpToPlaylists,
    JumpToLiked,
    JumpToArtists,

    // internal — not dispatched by key handler directly
    #[allow(dead_code)]
    EnterGMode,
    #[allow(dead_code)]
    ExitGMode,
}
