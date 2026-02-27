use anyhow::Result;
use rspotify::{model::FullTrack, prelude::*, AuthCodePkceSpotify};
use tracing::info;

// ─────────────────────────────────────────────────────────────────────────────
// Thin, typed structs so the rest of the app doesn't depend on rspotify types
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Fetch the current user's profile
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_user(spotify: &AuthCodePkceSpotify) -> Result<UserProfile> {
    let user = spotify.current_user().await?;
    Ok(UserProfile {
        display_name: user.display_name.unwrap_or_else(|| user.id.to_string()),
        id: user.id.to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch the user's playlists (all pages)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_playlists(spotify: &AuthCodePkceSpotify) -> Result<Vec<PlaylistSummary>> {
    let mut results = Vec::new();
    let mut stream = spotify.current_user_playlists();

    // rspotify async streams implement TryStreamExt
    use futures::StreamExt;
    while let Some(item) = stream.next().await {
        let playlist = item?;
        results.push(PlaylistSummary {
            id: playlist.id.to_string(),
            name: playlist.name.clone(),
            track_count: playlist.tracks.total,
        });
    }

    info!("Fetched {} playlists", results.len());
    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch tracks inside a specific playlist
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_playlist_tracks(
    spotify: &AuthCodePkceSpotify,
    playlist_id: &str,
) -> Result<Vec<TrackSummary>> {
    use futures::StreamExt;
    use rspotify::model::PlaylistId;

    let pid = PlaylistId::from_id(playlist_id)?;
    let mut results = Vec::new();
    let mut stream = spotify.playlist_items(pid, None, None);

    while let Some(item) = stream.next().await {
        let item = item?;
        if let Some(rspotify::model::PlayableItem::Track(track)) = item.track {
            results.push(track_to_summary(track));
        }
    }

    info!(
        "Fetched {} tracks for playlist {}",
        results.len(),
        playlist_id
    );
    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch the user's liked / saved tracks (all pages)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_liked_tracks(spotify: &AuthCodePkceSpotify) -> Result<Vec<TrackSummary>> {
    use futures::StreamExt;

    let mut results = Vec::new();
    let mut stream = spotify.current_user_saved_tracks(None);

    while let Some(item) = stream.next().await {
        let saved = item?;
        results.push(track_to_summary(saved.track));
    }

    info!("Fetched {} liked tracks", results.len());
    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch followed artists
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_followed_artists(spotify: &AuthCodePkceSpotify) -> Result<Vec<ArtistSummary>> {
    use rspotify::model::ArtistId;

    let mut results = Vec::new();
    let mut after: Option<String> = None;

    loop {
        let page = spotify
            .current_user_followed_artists(after.as_deref(), Some(50))
            .await?;

        for artist in &page.items {
            results.push(ArtistSummary {
                id: artist.id.to_string(),
                name: artist.name.clone(),
            });
        }

        if page.next.is_none() || page.items.is_empty() {
            break;
        }

        // The cursor for the next page is the last artist's id
        after = page.items.last().map(|a| a.id.to_string());
    }

    info!("Fetched {} followed artists", results.len());
    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch albums for a specific artist
// ─────────────────────────────────────────────────────────────────────────────

pub async fn fetch_artist_albums(
    spotify: &AuthCodePkceSpotify,
    artist_id: &str,
) -> Result<Vec<String>> {
    use futures::StreamExt;
    use rspotify::model::ArtistId;

    let aid = ArtistId::from_id(artist_id)?;
    let mut results = Vec::new();

    let mut stream = spotify.artist_albums(aid, None, None);
    while let Some(album) = stream.next().await {
        let album = album?;
        results.push(album.name.clone());
    }

    Ok(results)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

fn track_to_summary(track: FullTrack) -> TrackSummary {
    let artist = track
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();

    TrackSummary {
        id: track.id.map(|id| id.to_string()).unwrap_or_default(),
        name: track.name,
        artist,
        album: track.album.name,
        duration_ms: track.duration.num_milliseconds() as u32,
    }
}
