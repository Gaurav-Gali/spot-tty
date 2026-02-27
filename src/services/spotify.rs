//! Spotify data fetching via raw reqwest.

use anyhow::{bail, Context, Result};
use rspotify::{prelude::*, AuthCodePkceSpotify};
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

const BASE: &str = "https://api.spotify.com/v1";
const PAGE: u32 = 50;
const MAX_RETRIES: u32 = 4;

// ─────────────────────────────────────────────────────────────
// Public Types
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct UserProfile {
    pub display_name: String,
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub track_count: u32,
    pub owner: bool,
}

#[derive(Clone, Debug)]
pub struct TrackSummary {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
}

#[derive(Clone, Debug)]
pub struct ArtistSummary {
    pub id: String,
    pub name: String,
}

// ─────────────────────────────────────────────────────────────
// Raw JSON Shapes
// ─────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct Page<T> {
    items: Vec<T>,
    next: Option<String>,
}

#[derive(Deserialize, Debug)]
struct PlaylistPage {
    items: Vec<RawPlaylist>,
    next: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RawPlaylist {
    id: String,
    name: String,
    #[serde(rename = "items")]
    track_meta_new: Option<RawTrackMeta>,
    tracks: Option<RawTrackMeta>,
    owner: Option<RawOwner>,
}

#[derive(Deserialize, Debug)]
struct RawTrackMeta {
    total: u32,
}

#[derive(Deserialize, Debug)]
struct RawOwner {
    id: String,
}

#[derive(Deserialize, Debug)]
struct RawPlaylistItem {
    item: Option<RawTrack>,
    track: Option<RawTrack>,
}

#[derive(Deserialize, Clone, Debug)]
struct RawTrack {
    id: Option<String>,
    name: String,
    duration_ms: u32,
    artists: Vec<RawArtist>,
    album: Option<RawAlbum>,
}

#[derive(Deserialize, Clone, Debug)]
struct RawArtist {
    name: String,
}

#[derive(Deserialize, Clone, Debug)]
struct RawAlbum {
    name: String,
}

#[derive(Deserialize, Debug)]
struct RawSavedTrack {
    track: RawTrack,
}

#[derive(Deserialize, Debug)]
struct RawArtistObj {
    id: String,
    name: String,
}

#[derive(Deserialize, Debug)]
struct RawCursors {
    after: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RawCursorPage {
    items: Vec<RawArtistObj>,
    cursors: Option<RawCursors>,
}

#[derive(Deserialize, Debug)]
struct RawAlbumObj {
    name: String,
}

// ─────────────────────────────────────────────────────────────
// Token Helper
// ─────────────────────────────────────────────────────────────

async fn token(spotify: &AuthCodePkceSpotify) -> Result<String> {
    spotify.auto_reauth().await.ok();
    let guard = spotify.token.lock().await.unwrap();
    let tok = guard.as_ref().context("No token available")?;
    Ok(tok.access_token.clone())
}

// ─────────────────────────────────────────────────────────────
// HTTP GET with retry
// ─────────────────────────────────────────────────────────────

async fn get<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> Result<T> {
    let mut attempt = 0u32;

    loop {
        let resp = client.get(url).bearer_auth(access_token).send().await?;

        let status = resp.status();

        if status.as_u16() == 429 {
            attempt += 1;
            if attempt > MAX_RETRIES {
                bail!("429 after retries");
            }

            let retry_after = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);

            warn!("Rate limited. Waiting {retry_after}s");
            sleep(Duration::from_secs(retry_after + 1)).await;
            continue;
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("HTTP {status}: {body}");
        }

        let body = resp.text().await?;
        return Ok(serde_json::from_str(&body)?);
    }
}

// ─────────────────────────────────────────────────────────────
// Fetch User
// ─────────────────────────────────────────────────────────────

pub async fn fetch_user(spotify: &AuthCodePkceSpotify) -> Result<UserProfile> {
    #[derive(Deserialize)]
    struct Me {
        id: String,
        display_name: Option<String>,
    }

    let tok = token(spotify).await?;
    let client = reqwest::Client::new();
    let me: Me = get(&client, &format!("{BASE}/me"), &tok).await?;

    Ok(UserProfile {
        display_name: me.display_name.unwrap_or_else(|| me.id.clone()),
        id: me.id,
    })
}

// ─────────────────────────────────────────────────────────────
// Fetch Playlists
// ─────────────────────────────────────────────────────────────

pub async fn fetch_playlists(
    spotify: &AuthCodePkceSpotify,
    user_id: &str,
) -> Result<Vec<PlaylistSummary>> {
    let tok = token(spotify).await?;
    let client = reqwest::Client::new();

    let mut results = Vec::new();
    let mut offset = 0;

    loop {
        let url = format!("{BASE}/me/playlists?limit={PAGE}&offset={offset}");
        let page: PlaylistPage = get(&client, &url, &tok).await?;

        for pl in &page.items {
            let total = pl
                .track_meta_new
                .as_ref()
                .or(pl.tracks.as_ref())
                .map(|m| m.total)
                .unwrap_or(0);

            let owner = pl.owner.as_ref().map(|o| o.id == user_id).unwrap_or(false);

            results.push(PlaylistSummary {
                id: pl.id.clone(),
                name: pl.name.clone(),
                track_count: total,
                owner,
            });
        }

        offset += page.items.len() as u32;

        if page.next.is_none() || page.items.is_empty() {
            break;
        }
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────
// FIXED: Fetch Playlist Tracks
// ─────────────────────────────────────────────────────────────

pub async fn fetch_playlist_tracks(
    spotify: &AuthCodePkceSpotify,
    playlist_id: &str,
) -> Result<Vec<TrackSummary>> {
    let tok = token(spotify).await?;
    let client = reqwest::Client::new();

    let mut results = Vec::new();
    let mut offset = 0;

    loop {
        let url = format!("{BASE}/playlists/{playlist_id}/items?limit={PAGE}&offset={offset}");
        let page: Page<RawPlaylistItem> = get(&client, &url, &tok).await?;

        if page.items.is_empty() {
            break;
        }

        for item in &page.items {
            let track = item.item.as_ref().or(item.track.as_ref());
            if let Some(t) = track {
                results.push(raw_track_to_summary(t));
            }
        }

        offset += page.items.len() as u32;

        if page.next.is_none() {
            break;
        }
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────
// Fetch Liked
// ─────────────────────────────────────────────────────────────

pub async fn fetch_liked_tracks(spotify: &AuthCodePkceSpotify) -> Result<Vec<TrackSummary>> {
    let tok = token(spotify).await?;
    let client = reqwest::Client::new();

    let mut results = Vec::new();
    let mut offset = 0;

    loop {
        let url = format!("{BASE}/me/tracks?limit={PAGE}&offset={offset}");
        let page: Page<RawSavedTrack> = get(&client, &url, &tok).await?;

        if page.items.is_empty() {
            break;
        }

        for saved in &page.items {
            results.push(raw_track_to_summary(&saved.track));
        }

        offset += page.items.len() as u32;

        if page.next.is_none() {
            break;
        }
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────
// Fetch Followed Artists
// ─────────────────────────────────────────────────────────────

pub async fn fetch_followed_artists(spotify: &AuthCodePkceSpotify) -> Result<Vec<ArtistSummary>> {
    let tok = token(spotify).await?;
    let client = reqwest::Client::new();

    let mut results = Vec::new();
    let mut after: Option<String> = None;

    loop {
        let url = match &after {
            Some(c) => format!("{BASE}/me/following?type=artist&limit={PAGE}&after={c}"),
            None => format!("{BASE}/me/following?type=artist&limit={PAGE}"),
        };

        #[derive(Deserialize)]
        struct Wrapper {
            artists: RawCursorPage,
        }

        let wrapper: Wrapper = get(&client, &url, &tok).await?;
        let page = wrapper.artists;

        if page.items.is_empty() {
            break;
        }

        for a in &page.items {
            results.push(ArtistSummary {
                id: a.id.clone(),
                name: a.name.clone(),
            });
        }

        after = page.cursors.and_then(|c| c.after);

        if after.is_none() {
            break;
        }
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────
// Fetch Artist Albums
// ─────────────────────────────────────────────────────────────

pub async fn fetch_artist_albums(
    spotify: &AuthCodePkceSpotify,
    artist_id: &str,
) -> Result<Vec<String>> {
    let tok = token(spotify).await?;
    let client = reqwest::Client::new();

    let mut results = Vec::new();
    let mut offset = 0;

    loop {
        let url = format!(
            "{BASE}/artists/{artist_id}/albums?limit={PAGE}&offset={offset}&include_groups=album,single"
        );

        let page: Page<RawAlbumObj> = get(&client, &url, &tok).await?;

        if page.items.is_empty() {
            break;
        }

        for album in &page.items {
            results.push(album.name.clone());
        }

        offset += page.items.len() as u32;

        if page.next.is_none() {
            break;
        }
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────

fn raw_track_to_summary(t: &RawTrack) -> TrackSummary {
    TrackSummary {
        id: t.id.clone().unwrap_or_default(),
        name: t.name.clone(),
        artist: t
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
        duration_ms: t.duration_ms,
    }
}
