use crate::services::spotify::{Device, PlaybackState, PlaylistSummary, TrackSummary};
use crate::ui::cover::{CoverImage, ImageProtocol, RenderCache};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Clone)]
pub enum ExplorerNode {
    PlaylistTracks(String, String, bool),
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
    pub image_protocol: ImageProtocol,
    pub render_cache: RenderCache,

    pub loaded_user: bool,
    pub loaded_playlists: bool,
    pub loaded_liked: bool,
    pub explorer_fetch_pending: bool,

    pub user_name: Option<String>,
    pub playlists: Vec<PlaylistSummary>,
    pub liked_tracks: Vec<TrackSummary>,
    pub explorer_items: Vec<TrackSummary>,

    pub cover_cache: HashMap<String, CoverImage>,
    pub cover_fetching: HashSet<String>,

    pub navigation: NavigationState,
    pub explorer_stack: Vec<ExplorerNode>,
    pub explorer_selected_index: usize,
    pub key_mode: KeyMode,
    pub focus: Focus,
    pub pending_count: Option<usize>,

    pub error_message: Option<String>,
    pub visualizer_phase: usize,
    pub last_nav_move: Option<Instant>,

    // ── Playback ──────────────────────────────────────────────────────────────
    pub playback: Option<PlaybackState>,
    pub playing_context_uri: Option<String>,
    /// All available Spotify devices (refreshed on startup + after each play)
    pub devices: Vec<Device>,
}

impl AppState {
    pub fn scroll_settled(&self) -> bool {
        self.last_nav_move
            .map(|t| t.elapsed().as_millis() >= 120)
            .unwrap_or(true)
    }

    pub fn playback_progress(&self) -> f64 {
        match &self.playback {
            Some(p) if p.duration_ms > 0 => p.progress_ms as f64 / p.duration_ms as f64,
            _ => 0.0,
        }
    }

    pub fn is_playing_track(&self, track_id: &str) -> bool {
        self.playback
            .as_ref()
            .map(|p| p.track_id == track_id)
            .unwrap_or(false)
    }

    /// Active device first, then first available, then None.
    pub fn best_device_id(&self) -> Option<String> {
        // Prefer the device Spotify says is currently active
        if let Some(p) = &self.playback {
            if let Some(id) = &p.device_id {
                return Some(id.clone());
            }
        }
        self.devices
            .iter()
            .find(|d| d.is_active)
            .or_else(|| self.devices.first())
            .map(|d| d.id.clone())
    }
}
