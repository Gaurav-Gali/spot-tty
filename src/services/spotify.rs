//! Raw Spotify HTTP client (bypasses rspotify models; Feb-2026 API compatible).
use anyhow::{bail, Context, Result};
use rspotify::{prelude::*, AuthCodePkceSpotify};
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

const BASE: &str = "https://api.spotify.com/v1";
const PAGE: u32 = 50;
const MAX_RETRIES: u32 = 4;

// ── Public types ──────────────────────────────────────────────────────────────

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
    pub image_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TrackSummary {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub album_image_url: Option<String>,
    pub duration_ms: u32,
}

// ── Raw JSON shapes ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Page<T> {
    items: Vec<T>,
    next: Option<String>,
}
#[derive(Deserialize)]
struct PlaylistPage {
    #[serde(default)]
    items: Vec<RawPlaylist>,
    next: Option<String>,
}

#[derive(Deserialize)]
struct RawPlaylist {
    id: String,
    name: String,
    #[serde(rename = "items")]
    track_meta_new: Option<RawMeta>,
    tracks: Option<RawMeta>,
    owner: Option<RawOwner>,
    images: Option<Vec<RawImage>>,
}
#[derive(Deserialize)]
struct RawMeta {
    total: u32,
}
#[derive(Deserialize)]
struct RawOwner {
    id: String,
}
#[derive(Deserialize, Clone)]
struct RawImage {
    url: String,
    width: Option<u32>,
}

#[derive(Deserialize)]
struct RawPlaylistItem {
    item: Option<RawTrack>,
    track: Option<RawTrack>,
}
#[derive(Deserialize, Clone)]
struct RawTrack {
    id: Option<String>,
    name: String,
    duration_ms: u32,
    artists: Vec<RawArtist>,
    album: Option<RawAlbum>,
}
#[derive(Deserialize, Clone)]
struct RawArtist {
    name: String,
}
#[derive(Deserialize, Clone)]
struct RawAlbum {
    name: String,
    images: Option<Vec<RawImage>>,
}
#[derive(Deserialize)]
struct RawSaved {
    track: RawTrack,
}

// ── Token ─────────────────────────────────────────────────────────────────────

async fn token(sp: &AuthCodePkceSpotify) -> Result<String> {
    sp.auto_reauth().await.ok();
    let g = sp.token.lock().await.unwrap();
    Ok(g.as_ref().context("no token")?.access_token.clone())
}

// ── GET with 429 retry ────────────────────────────────────────────────────────

async fn get<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    tok: &str,
) -> Result<T> {
    let mut attempt = 0u32;
    loop {
        let resp = client.get(url).bearer_auth(tok).send().await?;
        let status = resp.status();
        if status.as_u16() == 429 {
            attempt += 1;
            if attempt > MAX_RETRIES {
                bail!("429 after retries: {url}");
            }
            let wait = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);
            warn!("Rate limited — waiting {wait}s");
            sleep(Duration::from_secs(wait + 1)).await;
            continue;
        }
        if !status.is_success() {
            bail!("HTTP {status}: {}", resp.text().await.unwrap_or_default());
        }
        let body = resp.text().await?;
        return serde_json::from_str(&body).with_context(|| format!("parsing {url}"));
    }
}

// ── Public fetch functions ────────────────────────────────────────────────────

pub async fn fetch_user(sp: &AuthCodePkceSpotify) -> Result<UserProfile> {
    #[derive(Deserialize)]
    struct Me {
        id: String,
        display_name: Option<String>,
    }
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let me: Me = get(&c, &format!("{BASE}/me"), &tok).await?;
    Ok(UserProfile {
        display_name: me.display_name.unwrap_or_else(|| me.id.clone()),
        id: me.id,
    })
}

pub async fn fetch_playlists(
    sp: &AuthCodePkceSpotify,
    user_id: &str,
) -> Result<Vec<PlaylistSummary>> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let mut results = vec![];
    let mut offset = 0u32;
    loop {
        let page: PlaylistPage = get(
            &c,
            &format!("{BASE}/me/playlists?limit={PAGE}&offset={offset}"),
            &tok,
        )
        .await?;
        for pl in &page.items {
            let total = pl
                .track_meta_new
                .as_ref()
                .or(pl.tracks.as_ref())
                .map(|m| m.total)
                .unwrap_or(0);
            results.push(PlaylistSummary {
                id: pl.id.clone(),
                name: pl.name.clone(),
                track_count: total,
                owner: pl.owner.as_ref().map(|o| o.id == user_id).unwrap_or(false),
                image_url: best_image(pl.images.as_deref()),
            });
        }
        offset += page.items.len() as u32;
        if page.next.is_none() || page.items.is_empty() {
            break;
        }
    }
    info!("playlists: {}", results.len());
    Ok(results)
}

pub async fn fetch_playlist_tracks(
    sp: &AuthCodePkceSpotify,
    id: &str,
) -> Result<Vec<TrackSummary>> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let mut results = vec![];
    let mut offset = 0u32;
    loop {
        let url = format!("{BASE}/playlists/{id}/items?limit={PAGE}&offset={offset}");
        let page: Page<RawPlaylistItem> = match get(&c, &url, &tok).await {
            Ok(p) => p,
            Err(e) if e.to_string().contains("403") => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        if page.items.is_empty() {
            break;
        }
        for item in &page.items {
            if let Some(t) = item.item.as_ref().or(item.track.as_ref()) {
                results.push(to_summary(t));
            }
        }
        offset += page.items.len() as u32;
        if page.next.is_none() {
            break;
        }
    }
    info!("tracks for {id}: {}", results.len());
    Ok(results)
}

pub async fn fetch_liked_tracks(sp: &AuthCodePkceSpotify) -> Result<Vec<TrackSummary>> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let mut results = vec![];
    let mut offset = 0u32;
    loop {
        let page: Page<RawSaved> = get(
            &c,
            &format!("{BASE}/me/tracks?limit={PAGE}&offset={offset}"),
            &tok,
        )
        .await?;
        if page.items.is_empty() {
            break;
        }
        for s in &page.items {
            results.push(to_summary(&s.track));
        }
        offset += page.items.len() as u32;
        if page.next.is_none() {
            break;
        }
    }
    info!("liked: {}", results.len());
    Ok(results)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn best_image(images: Option<&[RawImage]>) -> Option<String> {
    let imgs = images?;
    if imgs.is_empty() {
        return None;
    }
    // Prefer smallest image ≥ 300 px (good source for Lanczos downscaling).
    // Spotify returns images largest→smallest; 300px is usually the middle one.
    // Fall back to first (largest) if nothing ≥ 300px found.
    imgs.iter()
        .filter(|i| i.width.map(|w| w >= 300).unwrap_or(true))
        .min_by_key(|i| i.width.unwrap_or(9999))
        .or_else(|| imgs.first())
        .map(|i| i.url.clone())
}

fn to_summary(t: &RawTrack) -> TrackSummary {
    TrackSummary {
        id: t.id.clone().unwrap_or_default(),
        name: t.name.clone(),
        artist: t
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
        album_image_url: t
            .album
            .as_ref()
            .and_then(|a| best_image(a.images.as_deref())),
        duration_ms: t.duration_ms,
    }
}
