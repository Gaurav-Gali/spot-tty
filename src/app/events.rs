use crate::services::spotify::{Device, PlaybackState, PlaylistSummary, TrackSummary};
use crate::ui::cover::CoverImage;

pub enum AppEvent {
    Quit,
    UserLoaded(String),
    PlaylistsLoaded(Vec<PlaylistSummary>),
    LikedTracksLoaded(Vec<TrackSummary>),
    ExplorerTracksLoaded(Vec<TrackSummary>),
    CoverLoaded(String, CoverImage),
    LoadError(String),
    // Navigation
    MoveDown(usize),
    MoveUp(usize),
    GoTop,
    GoBottom,
    GoMiddle,
    Enter,
    Back,
    JumpToPlaylists,
    JumpToLiked,
    // Playback
    PlayTrack {
        track: TrackSummary,
        context_uri: Option<String>,
    },
    TogglePause,
    PlaybackStateUpdated(Option<PlaybackState>),
    DevicesUpdated(Vec<Device>),
}
