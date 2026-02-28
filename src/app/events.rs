use crate::services::spotify::{PlaylistSummary, TrackSummary};
use crate::ui::cover::CoverImage;

pub enum AppEvent {
    Quit,
    UserLoaded(String),
    PlaylistsLoaded(Vec<PlaylistSummary>),
    LikedTracksLoaded(Vec<TrackSummary>),
    ExplorerTracksLoaded(Vec<TrackSummary>),
    CoverLoaded(String, CoverImage),
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
