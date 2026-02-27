use std::io;
use std::time::Instant;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{backend::CrosstermBackend, Terminal};
use rspotify::AuthCodePkceSpotify;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging
    let log_file = std::fs::File::create("/tmp/spot-tty.log")?;
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    // Config
    let settings = Settings::load()?;

    // Auth
    let spotify: AuthCodePkceSpotify = auth::authenticate(
        &settings.client_id,
        &settings.client_secret,
        &settings.redirect_uri,
    )
    .await?;

    // TUI setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new();

    spawn_initial_fetches(spotify.clone(), tx.clone());

    let tick_rate = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    let mut last_fetched_stack: Option<ExplorerNode> = None;
    let mut explorer_fetch_in_progress = false;

    loop {
        // Animation tick
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            app.state.playback_progress += 0.01;
            if app.state.playback_progress > 1.0 {
                app.state.playback_progress = 0.0;
            }
            app.state.visualizer_phase = (app.state.visualizer_phase + 1) % 1000;
        }

        // Draw
        terminal.draw(|frame| {
            let areas = ui::layout::split(frame.size());
            ui::sidebar::render(frame, areas.sidebar, &app.state);
            ui::explorer::render(frame, areas.main, &app.state);
            ui::player::render(frame, areas.control, &app.state);
        })?;

        // Drain async events
        while let Ok(event) = rx.try_recv() {
            match &event {
                AppEvent::ExplorerTracksLoaded(_) | AppEvent::ExplorerAlbumsLoaded(_) => {
                    explorer_fetch_in_progress = false;
                }
                _ => {}
            }
            app.handle_event(event);
        }

        // Lazy explorer fetch
        maybe_fetch_explorer(
            &app.state.explorer_stack.last().cloned(),
            &mut last_fetched_stack,
            &mut explorer_fetch_in_progress,
            spotify.clone(),
            tx.clone(),
        );

        // Input
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
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

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn spawn_initial_fetches(spotify: AuthCodePkceSpotify, tx: mpsc::UnboundedSender<AppEvent>) {
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_user(&sp).await {
                Ok(user) => {
                    let user_id = user.id.clone();
                    let _ = tx.send(AppEvent::UserLoaded(user.display_name));

                    match svc::fetch_playlists(&sp, &user_id).await {
                        Ok(pl) => {
                            let _ = tx.send(AppEvent::PlaylistsLoaded(pl));
                        }
                        Err(e) => {
                            tracing::error!("fetch_playlists failed: {e:#}");
                            let _ = tx.send(AppEvent::PlaylistsLoaded(vec![]));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("fetch_user failed: {e:#}");
                    let _ = tx.send(AppEvent::LoadError(format!("Profile: {e}")));
                    let _ = tx.send(AppEvent::PlaylistsLoaded(vec![]));
                }
            }
        });
    }

    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            sleep(Duration::from_millis(300)).await;
            match svc::fetch_liked_tracks(&sp).await {
                Ok(tracks) => {
                    let _ = tx.send(AppEvent::LikedTracksLoaded(tracks));
                }
                Err(e) => {
                    tracing::error!("fetch_liked_tracks failed: {e:#}");
                    let _ = tx.send(AppEvent::LikedTracksLoaded(vec![]));
                }
            }
        });
    }

    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            sleep(Duration::from_millis(600)).await;
            match svc::fetch_followed_artists(&sp).await {
                Ok(artists) => {
                    let _ = tx.send(AppEvent::ArtistsLoaded(artists));
                }
                Err(e) => {
                    tracing::error!("fetch_followed_artists failed: {e:#}");
                    let _ = tx.send(AppEvent::ArtistsLoaded(vec![]));
                }
            }
        });
    }
}

fn maybe_fetch_explorer(
    current: &Option<ExplorerNode>,
    last: &mut Option<ExplorerNode>,
    in_progress: &mut bool,
    spotify: AuthCodePkceSpotify,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    if *in_progress {
        return;
    }

    let should_fetch = match (current, last.as_ref()) {
        (None, _) => false,
        (Some(curr), None) => !matches!(curr, ExplorerNode::LikedTracks),
        (Some(curr), Some(prev)) => !nodes_equal(curr, prev),
    };

    if !should_fetch {
        return;
    }

    *last = current.clone();
    *in_progress = true;

    match current {
        Some(ExplorerNode::PlaylistTracks(id, _, _)) => {
            let id = id.clone();
            tokio::spawn(async move {
                match svc::fetch_playlist_tracks(&spotify, &id).await {
                    Ok(t) => {
                        let _ = tx.send(AppEvent::ExplorerTracksLoaded(t));
                    }
                    Err(e) => {
                        tracing::error!("fetch_playlist_tracks: {e:#}");
                        let _ = tx.send(AppEvent::ExplorerTracksLoaded(vec![]));
                    }
                }
            });
        }

        Some(ExplorerNode::ArtistAlbums(id, _)) => {
            let id = id.clone();
            tokio::spawn(async move {
                match svc::fetch_artist_albums(&spotify, &id).await {
                    Ok(a) => {
                        let _ = tx.send(AppEvent::ExplorerAlbumsLoaded(a));
                    }
                    Err(e) => {
                        tracing::error!("fetch_artist_albums: {e:#}");
                        let _ = tx.send(AppEvent::ExplorerAlbumsLoaded(vec![]));
                    }
                }
            });
        }

        _ => {
            *in_progress = false;
        }
    }
}

fn nodes_equal(a: &ExplorerNode, b: &ExplorerNode) -> bool {
    match (a, b) {
        (ExplorerNode::PlaylistTracks(id1, _, _), ExplorerNode::PlaylistTracks(id2, _, _)) => {
            id1 == id2
        }
        (ExplorerNode::ArtistAlbums(id1, _), ExplorerNode::ArtistAlbums(id2, _)) => id1 == id2,
        (ExplorerNode::LikedTracks, ExplorerNode::LikedTracks) => true,
        _ => false,
    }
}
