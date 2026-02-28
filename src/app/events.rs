use crate::services::spotify::{PlaylistSummary, TrackSummary};
use crate::ui::cover::CoverArt;

pub enum AppEvent {
    Quit,
    UserLoaded(String),
    PlaylistsLoaded(Vec<PlaylistSummary>),
    LikedTracksLoaded(Vec<TrackSummary>),
    ExplorerTracksLoaded(Vec<TrackSummary>),
    /// (url, small_art, large_art)
    CoverLoaded(String, CoverArt, CoverArt),
    LoadError(String),
    MoveDown(usize),
    MoveUp(usize),
    GoTop,
    GoBottom,
    GoMiddle,
    Enter,
    Back,
    JumpToPlaylists,
    JumpToLiked,
}
