use crate::services::spotify::UserProfile;
use crate::services::spotify::{Device, PlaybackState, PlaylistSummary, TrackSummary};
use crate::ui::cover::{CoverImage, ImageProtocol, RenderCache};
use crate::ui::profile::ProfileState;
use crate::ui::search::SearchState;
use crate::ui::trackmenu::TrackMenuState;
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
    Search,
    TrackMenu,
    Profile,
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
    pub devices: Vec<Device>,

    // ── Overlays ──────────────────────────────────────────────────────────────
    pub search: SearchState,
    pub track_menu: TrackMenuState,
    pub profile: ProfileState,
    pub user_profile: Option<UserProfile>,

    /// Flat list of all tracks across liked + all loaded playlists — for search
    pub all_tracks: Vec<TrackSummary>,

    /// Toast notification shown bottom-right after actions
    pub toast: Option<(String, Instant)>, // (message, shown_at)
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

    pub fn best_device_id(&self) -> Option<String> {
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

    /// Merge new tracks into all_tracks, deduplicating by id
    pub fn merge_tracks(&mut self, tracks: &[TrackSummary]) {
        let existing: HashSet<String> = self.all_tracks.iter().map(|t| t.id.clone()).collect();
        for t in tracks {
            if !t.id.is_empty() && !existing.contains(&t.id) {
                self.all_tracks.push(t.clone());
            }
        }
    }

    /// Show a toast message for 3 seconds
    pub fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Returns active toast if it hasn't expired (3 s TTL)
    pub fn active_toast(&self) -> Option<&str> {
        self.toast.as_ref().and_then(|(msg, t)| {
            if t.elapsed().as_secs() < 3 {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }
}
