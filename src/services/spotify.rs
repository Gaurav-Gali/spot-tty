//! Spotify HTTP client
use anyhow::{bail, Context, Result};
use rspotify::{prelude::*, AuthCodePkceSpotify};
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

const BASE: &str = "https://api.spotify.com/v1";
const PAGE: u32 = 50;
const MAX_RETRIES: u32 = 4;

#[derive(Clone, Debug, Default)]
pub struct UserProfile {
    pub display_name: String,
    pub id: String,
    pub email: Option<String>,
    pub country: Option<String>,
    pub product: Option<String>, // "premium" | "free"
    pub followers: u32,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UserStats {
    pub total_liked: u32,
    pub total_playlists: u32,
    pub owned_playlists: u32,
    pub unique_artists: u32,
    pub unique_albums: u32,
    pub total_duration_ms: u64,
    pub top_artists: Vec<String>, // derived from liked tracks
    pub top_albums: Vec<String>,
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

async fn token(sp: &AuthCodePkceSpotify) -> Result<String> {
    sp.auto_reauth().await.ok();
    let g = sp.token.lock().await.unwrap();
    Ok(g.as_ref().context("no token")?.access_token.clone())
}

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

pub async fn fetch_user(sp: &AuthCodePkceSpotify) -> Result<UserProfile> {
    #[derive(Deserialize)]
    struct Me {
        id: String,
        display_name: Option<String>,
        email: Option<String>,
        country: Option<String>,
        product: Option<String>,
        followers: Option<Followers>,
        images: Option<Vec<RawImage>>,
    }
    #[derive(Deserialize)]
    struct Followers {
        total: u32,
    }
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let me: Me = get(&c, &format!("{BASE}/me"), &tok).await?;
    Ok(UserProfile {
        display_name: me.display_name.clone().unwrap_or_else(|| me.id.clone()),
        id: me.id,
        email: me.email,
        country: me.country,
        product: me.product,
        followers: me.followers.map(|f| f.total).unwrap_or(0),
        avatar_url: best_image(me.images.as_deref()),
    })
}

pub fn compute_stats(
    _profile: &UserProfile,
    playlists: &[PlaylistSummary],
    liked: &[TrackSummary],
) -> UserStats {
    use std::collections::HashMap;
    let mut artist_counts: HashMap<&str, u32> = HashMap::new();
    let mut album_counts: HashMap<&str, u32> = HashMap::new();
    let mut total_ms = 0u64;

    for t in liked {
        *artist_counts.entry(t.artist.as_str()).or_default() += 1;
        *album_counts.entry(t.album.as_str()).or_default() += 1;
        total_ms += t.duration_ms as u64;
    }

    let mut artists: Vec<(&str, u32)> = artist_counts.into_iter().collect();
    artists.sort_by(|a, b| b.1.cmp(&a.1));
    let mut albums: Vec<(&str, u32)> = album_counts.into_iter().collect();
    albums.sort_by(|a, b| b.1.cmp(&a.1));

    UserStats {
        total_liked: liked.len() as u32,
        total_playlists: playlists.len() as u32,
        owned_playlists: playlists.iter().filter(|p| p.owner).count() as u32,
        unique_artists: artists.len() as u32,
        unique_albums: albums.len() as u32,
        total_duration_ms: total_ms,
        top_artists: artists
            .into_iter()
            .take(8)
            .map(|(n, _)| n.to_string())
            .collect(),
        top_albums: albums
            .into_iter()
            .take(5)
            .map(|(n, _)| n.to_string())
            .collect(),
    }
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

pub struct PlaybackState {
    pub track_id: String,
    pub track_name: String,
    pub artist: String,
    pub album: String,
    pub album_image_url: Option<String>,
    pub duration_ms: u32,
    pub progress_ms: u32,
    pub is_playing: bool,
    pub device_id: Option<String>,
}

pub async fn play_track(
    sp: &AuthCodePkceSpotify,
    track_uri: &str,
    context_uri: Option<&str>,
    device_id: Option<&str>,
) -> Result<()> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();

    let body = match context_uri {
        Some(ctx) => serde_json::json!({
            "context_uri": ctx,
            "offset": { "uri": track_uri }
        }),
        None => serde_json::json!({
            "uris": [track_uri]
        }),
    };

    let url = match device_id {
        Some(d) => format!("{BASE}/me/player/play?device_id={d}"),
        None => format!("{BASE}/me/player/play"),
    };

    put_no_body(&c, &url, &tok, Some(body)).await
}

/// PUT /me/player/pause
pub async fn pause(sp: &AuthCodePkceSpotify) -> Result<()> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    put_no_body(&c, &format!("{BASE}/me/player/pause"), &tok, None).await
}

pub async fn resume(sp: &AuthCodePkceSpotify) -> Result<()> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    put_no_body(&c, &format!("{BASE}/me/player/play"), &tok, None).await
}

pub async fn fetch_playback_state(sp: &AuthCodePkceSpotify) -> Result<Option<PlaybackState>> {
    #[derive(Deserialize)]
    struct Player {
        is_playing: bool,
        progress_ms: Option<u32>,
        item: Option<PlayerTrack>,
        device: Option<PlayerDevice>,
    }
    #[derive(Deserialize)]
    struct PlayerTrack {
        id: Option<String>,
        name: String,
        duration_ms: u32,
        artists: Vec<RawArtist>,
        album: Option<RawAlbum>,
    }
    #[derive(Deserialize)]
    struct PlayerDevice {
        id: Option<String>,
    }

    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let resp = c
        .get(&format!("{BASE}/me/player"))
        .bearer_auth(&tok)
        .send()
        .await?;

    if resp.status().as_u16() == 204 {
        return Ok(None);
    } // no active device
    if !resp.status().is_success() {
        return Ok(None);
    }

    let text = resp.text().await?;
    if text.is_empty() {
        return Ok(None);
    }

    let player: Player = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let Some(item) = player.item else {
        return Ok(None);
    };

    Ok(Some(PlaybackState {
        track_id: item.id.clone().unwrap_or_default(),
        track_name: item.name.clone(),
        artist: item
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        album: item
            .album
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        album_image_url: item
            .album
            .as_ref()
            .and_then(|a| best_image(a.images.as_deref())),
        duration_ms: item.duration_ms,
        progress_ms: player.progress_ms.unwrap_or(0),
        is_playing: player.is_playing,
        device_id: player.device.and_then(|d| d.id),
    }))
}

async fn put_no_body(
    client: &reqwest::Client,
    url: &str,
    tok: &str,
    body: Option<serde_json::Value>,
) -> Result<()> {
    let mut req = client
        .put(url)
        .bearer_auth(tok)
        .header("Content-Type", "application/json");
    req = match body {
        Some(b) => req.body(b.to_string()),
        None => req.header("Content-Length", "0"),
    };
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    match status {
        200 | 204 => Ok(()),
        404 => {
            // No active device — common; not fatal
            warn!("Playback: no active device (404)");
            Ok(())
        }
        _ => bail!(
            "PUT {url} → HTTP {status}: {}",
            resp.text().await.unwrap_or_default()
        ),
    }
}

pub async fn skip_next(sp: &AuthCodePkceSpotify) -> Result<()> {
    let tok = token(sp).await?;
    let resp = reqwest::Client::new()
        .post(format!("{BASE}/me/player/next"))
        .bearer_auth(&tok)
        .header("Content-Length", "0")
        .send()
        .await?;
    let s = resp.status().as_u16();
    match s {
        200 | 204 => Ok(()),
        404 => {
            warn!("skip_next: no active device");
            Ok(())
        }
        _ => bail!(
            "POST /me/player/next → {s}: {}",
            resp.text().await.unwrap_or_default()
        ),
    }
}

/// POST /me/player/previous — skip to previous track
pub async fn skip_prev(sp: &AuthCodePkceSpotify) -> Result<()> {
    let tok = token(sp).await?;
    let resp = reqwest::Client::new()
        .post(format!("{BASE}/me/player/previous"))
        .bearer_auth(&tok)
        .header("Content-Length", "0")
        .send()
        .await?;
    let s = resp.status().as_u16();
    match s {
        200 | 204 => Ok(()),
        404 => {
            warn!("skip_prev: no active device");
            Ok(())
        }
        _ => bail!(
            "POST /me/player/previous → {s}: {}",
            resp.text().await.unwrap_or_default()
        ),
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

pub async fn fetch_devices(sp: &AuthCodePkceSpotify) -> Result<Vec<Device>> {
    #[derive(Deserialize)]
    struct Resp {
        devices: Vec<RawDevice>,
    }
    #[derive(Deserialize)]
    struct RawDevice {
        id: Option<String>,
        name: String,
        is_active: bool,
    }
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let resp: Resp = get(&c, &format!("{BASE}/me/player/devices"), &tok).await?;
    Ok(resp
        .devices
        .into_iter()
        .filter_map(|d| {
            d.id.map(|id| Device {
                id,
                name: d.name,
                is_active: d.is_active,
            })
        })
        .collect())
}

pub async fn add_to_queue(sp: &AuthCodePkceSpotify, track_id: &str) -> Result<()> {
    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let url = format!("{BASE}/me/player/queue?uri=spotify:track:{track_id}");
    let resp = c
        .post(&url)
        .bearer_auth(&tok)
        .header("Content-Length", "0")
        .send()
        .await?;
    let s = resp.status().as_u16();
    if s == 200 || s == 204 {
        Ok(())
    } else {
        bail!(
            "POST /me/player/queue → {s}: {}",
            resp.text().await.unwrap_or_default()
        )
    }
}

pub async fn fetch_all_tracks_for_search(sp: &AuthCodePkceSpotify) -> Result<Vec<TrackSummary>> {
    fetch_liked_tracks(sp).await
}

pub async fn search_tracks(
    sp: &AuthCodePkceSpotify,
    query: &str,
    limit: u32,
) -> Result<Vec<TrackSummary>> {
    #[derive(Deserialize)]
    struct SearchResp {
        tracks: TrackPage,
    }
    #[derive(Deserialize)]
    struct TrackPage {
        items: Vec<RawTrack>,
    }

    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let tok = token(sp).await?;
    let c = reqwest::Client::new();
    let url = format!(
        "{BASE}/search?q={}&type=track&limit={limit}",
        urlencoding::encode(query)
    );

    let resp: SearchResp = match get(&c, &url, &tok).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("search_tracks: {e:#}");
            return Ok(vec![]);
        }
    };

    Ok(resp.tracks.items.iter().map(to_summary).collect())
}
