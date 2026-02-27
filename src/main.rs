use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rspotify::AuthCodePkceSpotify;
use tokio::sync::mpsc;

mod app;
mod cache;
mod config;
mod navigation;
mod services;
mod ui;

use app::{
    app::App,
    events::AppEvent,
    state::{ExplorerNode, KeyMode},
};
use config::settings::Settings;
use services::{auth, spotify as svc};

// ─────────────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging (writes to a file so it doesn't pollute the TUI) ─────────
    let log_file = std::fs::File::create("/tmp/spot-tty.log")?;
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    // ── Load config ───────────────────────────────────────────────────────
    let settings = Settings::load()?;

    // ── Auth (before entering raw mode so the browser URL prints cleanly) ─
    let spotify: AuthCodePkceSpotify = auth::authenticate(
        &settings.client_id,
        &settings.client_secret,
        &settings.redirect_uri,
    )
    .await?;

    // ── Enter TUI ─────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new();

    // ── Kick off initial parallel data fetches ────────────────────────────
    spawn_initial_fetches(spotify.clone(), tx.clone());

    let tick_rate = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    // Track what the last sidebar selection was so we only re-fetch when it
    // changes (avoid hammering Spotify on every key event).
    let mut last_fetched_stack: Option<ExplorerNode> = None;

    loop {
        // ── Animation tick ────────────────────────────────────────────────
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            app.state.playback_progress += 0.01;
            if app.state.playback_progress > 1.0 {
                app.state.playback_progress = 0.0;
            }
            app.state.visualizer_phase = (app.state.visualizer_phase + 1) % 1000;
        }

        // ── Draw ──────────────────────────────────────────────────────────
        terminal.draw(|frame| {
            let areas = ui::layout::split(frame.size());
            ui::sidebar::render(frame, areas.sidebar, &app.state);
            ui::explorer::render(frame, areas.main, &app.state);
            ui::player::render(frame, areas.control, &app.state);
        })?;

        // ── Drain event queue ─────────────────────────────────────────────
        while let Ok(event) = rx.try_recv() {
            app.handle_event(event);
        }

        // ── Lazy explorer fetch: triggered when the sidebar selection changes ──
        maybe_fetch_explorer(
            &app.state.explorer_stack.last().cloned(),
            &mut last_fetched_stack,
            spotify.clone(),
            tx.clone(),
        );

        // ── Input ─────────────────────────────────────────────────────────
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                // Digit accumulation for count prefix (e.g. 5j)
                if let KeyCode::Char(c) = key.code {
                    if c.is_ascii_digit() {
                        let digit = c.to_digit(10).unwrap() as usize;
                        let current = app.state.pending_count.unwrap_or(0);
                        app.state.pending_count = Some(current * 10 + digit);
                        continue;
                    }
                }

                let count = app.state.pending_count.take().unwrap_or(1);

                match app.state.key_mode {
                    KeyMode::Normal => match key.code {
                        KeyCode::Char('j') | KeyCode::Down => tx.send(AppEvent::MoveDown(count))?,
                        KeyCode::Char('k') | KeyCode::Up => tx.send(AppEvent::MoveUp(count))?,
                        KeyCode::Char('G') => tx.send(AppEvent::GoBottom)?,
                        KeyCode::Char('M') => tx.send(AppEvent::GoMiddle)?,
                        KeyCode::Char('g') => app.state.key_mode = KeyMode::AwaitingG,
                        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                            tx.send(AppEvent::Enter)?
                        }
                        KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                            tx.send(AppEvent::Back)?
                        }
                        KeyCode::Char('q') => tx.send(AppEvent::Quit)?,
                        _ => {}
                    },
                    KeyMode::AwaitingG => {
                        match key.code {
                            KeyCode::Char('g') => tx.send(AppEvent::GoTop)?,
                            KeyCode::Char('p') => tx.send(AppEvent::JumpToPlaylists)?,
                            KeyCode::Char('l') => tx.send(AppEvent::JumpToLiked)?,
                            KeyCode::Char('a') => tx.send(AppEvent::JumpToArtists)?,
                            _ => {}
                        }
                        app.state.key_mode = KeyMode::Normal;
                    }
                }
            }
        }

        if app.state.should_quit {
            break;
        }
    }

    // ── Restore terminal ──────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Spawn the four parallel initial fetches as separate tasks
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_initial_fetches(spotify: AuthCodePkceSpotify, tx: mpsc::UnboundedSender<AppEvent>) {
    // User profile
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_user(&sp).await {
                Ok(user) => {
                    let _ = tx.send(AppEvent::UserLoaded(user.display_name));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::LoadError(format!("Profile: {e}")));
                }
            }
        });
    }

    // Playlists
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_playlists(&sp).await {
                Ok(pl) => {
                    let _ = tx.send(AppEvent::PlaylistsLoaded(pl));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::LoadError(format!("Playlists: {e}")));
                }
            }
        });
    }

    // Liked tracks
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_liked_tracks(&sp).await {
                Ok(tracks) => {
                    let _ = tx.send(AppEvent::LikedTracksLoaded(tracks));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::LoadError(format!("Liked: {e}")));
                }
            }
        });
    }

    // Followed artists
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_followed_artists(&sp).await {
                Ok(artists) => {
                    let _ = tx.send(AppEvent::ArtistsLoaded(artists));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::LoadError(format!("Artists: {e}")));
                }
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lazy explorer fetch — fires only when the selected node changes
// ─────────────────────────────────────────────────────────────────────────────

fn maybe_fetch_explorer(
    current: &Option<ExplorerNode>,
    last: &mut Option<ExplorerNode>,
    spotify: AuthCodePkceSpotify,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let should_fetch = match (current, last.as_ref()) {
        (None, _) => false,
        (Some(curr), None) => !matches!(curr, ExplorerNode::LikedTracks),
        (Some(curr), Some(prev)) => !nodes_equal(curr, prev),
    };

    if !should_fetch {
        return;
    }

    *last = current.clone();

    match current {
        Some(ExplorerNode::PlaylistTracks(id, _)) => {
            let id = id.clone();
            tokio::spawn(async move {
                match svc::fetch_playlist_tracks(&spotify, &id).await {
                    Ok(tracks) => {
                        let _ = tx.send(AppEvent::ExplorerTracksLoaded(tracks));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::LoadError(format!("Playlist tracks: {e}")));
                    }
                }
            });
        }
        Some(ExplorerNode::ArtistAlbums(id, _)) => {
            let id = id.clone();
            tokio::spawn(async move {
                match svc::fetch_artist_albums(&spotify, &id).await {
                    Ok(albums) => {
                        let _ = tx.send(AppEvent::ExplorerAlbumsLoaded(albums));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::LoadError(format!("Artist albums: {e}")));
                    }
                }
            });
        }
        // LikedTracks are already in state from the initial fetch — reducer handles it
        _ => {}
    }
}

fn nodes_equal(a: &ExplorerNode, b: &ExplorerNode) -> bool {
    match (a, b) {
        (ExplorerNode::PlaylistTracks(id1, _), ExplorerNode::PlaylistTracks(id2, _)) => id1 == id2,
        (ExplorerNode::ArtistAlbums(id1, _), ExplorerNode::ArtistAlbums(id2, _)) => id1 == id2,
        (ExplorerNode::LikedTracks, ExplorerNode::LikedTracks) => true,
        _ => false,
    }
}
