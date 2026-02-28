use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rspotify::AuthCodePkceSpotify;
use std::{io, time::Instant};
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
    state::{ExplorerNode, Focus, KeyMode},
};
use config::settings::Settings;
use services::{auth, spotify as svc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log = std::fs::File::create("/tmp/spot-tty.log")?;
    tracing_subscriber::fmt()
        .with_writer(log)
        .with_ansi(false)
        .init();

    let settings = Settings::load()?;
    let spotify = auth::authenticate(
        &settings.client_id,
        &settings.client_secret,
        &settings.redirect_uri,
    )
    .await?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new();
    spawn_initial_fetches(spotify.clone(), tx.clone());

    let tick_rate = Duration::from_millis(150);
    let poll_rate = Duration::from_secs(2);
    let mut last_tick = Instant::now();
    let mut last_poll = Instant::now();
    let mut last_node: Option<ExplorerNode> = None;
    let mut fetch_in_progress = false;

    loop {
        // ── Tick ──────────────────────────────────────────────────────────────
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            if app
                .state
                .playback
                .as_ref()
                .map(|p| p.is_playing)
                .unwrap_or(false)
            {
                app.state.visualizer_phase = (app.state.visualizer_phase + 1) % 100;
                if let Some(p) = &mut app.state.playback {
                    p.progress_ms = (p.progress_ms + 150).min(p.duration_ms);
                }
            }
        }

        // ── Poll playback state every 2 s ─────────────────────────────────────
        if last_poll.elapsed() >= poll_rate {
            last_poll = Instant::now();
            let sp = spotify.clone();
            let tx2 = tx.clone();
            tokio::spawn(async move {
                match svc::fetch_playback_state(&sp).await {
                    Ok(s) => {
                        let _ = tx2.send(AppEvent::PlaybackStateUpdated(s));
                    }
                    Err(e) => tracing::warn!("playback poll: {e:#}"),
                }
            });
        }

        // ── Lazy cover fetching ───────────────────────────────────────────────
        {
            let size = terminal.size().unwrap_or_default();
            let areas = ui::layout::split(size);
            let sidebar_urls: Vec<String> = {
                let sel = app.state.navigation.selected_index;
                let vis = (areas.sidebar.height.saturating_sub(8) / 4) as usize;
                let scroll = sel.saturating_sub(vis.saturating_sub(1));
                app.state
                    .playlists
                    .iter()
                    .skip(scroll)
                    .take(vis + 2)
                    .filter_map(|p| p.image_url.clone())
                    .collect()
            };
            let mut all_urls = ui::explorer::visible_cover_urls(&app.state, areas.main);
            for u in sidebar_urls {
                if !all_urls.contains(&u) {
                    all_urls.push(u);
                }
            }
            if let Some(url) = app
                .state
                .playback
                .as_ref()
                .and_then(|p| p.album_image_url.as_ref())
            {
                if !all_urls.contains(url) {
                    all_urls.insert(0, url.clone());
                }
            }
            for url in all_urls {
                if !app.state.cover_cache.contains_key(&url)
                    && !app.state.cover_fetching.contains(&url)
                {
                    app.state.cover_fetching.insert(url.clone());
                    let tx2 = tx.clone();
                    tokio::spawn(async move {
                        if let Some(img) = ui::cover::fetch_cover(&url).await {
                            let _ = tx2.send(AppEvent::CoverLoaded(url, img));
                        }
                    });
                }
            }
        }

        // ── Render ────────────────────────────────────────────────────────────
        app.state.render_cache.begin_frame();
        let cache_ptr = &mut app.state.render_cache as *mut _;
        terminal.draw(|f| {
            let cache = unsafe { &mut *cache_ptr };
            let areas = ui::layout::split(f.size());
            ui::sidebar::render(f, areas.sidebar, &app.state, cache);
            ui::explorer::render(f, areas.main, &app.state, cache);
            ui::player::render(f, areas.control, &app.state);
        })?;
        app.state.render_cache.flush();

        // ── Events ────────────────────────────────────────────────────────────
        while let Ok(ev) = rx.try_recv() {
            match &ev {
                AppEvent::ExplorerTracksLoaded(_) => {
                    fetch_in_progress = false;
                }
                AppEvent::LoadError(_) => {
                    fetch_in_progress = false;
                }
                _ => {}
            }
            app.handle_event(ev);
        }

        if app.state.explorer_fetch_pending && !fetch_in_progress {
            last_node = None;
        }
        maybe_fetch_explorer(
            &app.state.explorer_stack.last().cloned(),
            &mut last_node,
            &mut fetch_in_progress,
            spotify.clone(),
            tx.clone(),
        );

        // ── Input ─────────────────────────────────────────────────────────────
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                // Digit prefix accumulation
                if let KeyCode::Char(c) = key.code {
                    if c.is_ascii_digit() {
                        let d = c.to_digit(10).unwrap() as usize;
                        app.state.pending_count =
                            Some(app.state.pending_count.unwrap_or(0) * 10 + d);
                        continue;
                    }
                }
                let count = app.state.pending_count.take().unwrap_or(1);

                match app.state.key_mode {
                    KeyMode::Normal => {
                        match key.code {
                            // Motion
                            KeyCode::Char('j') | KeyCode::Down => {
                                tx.send(AppEvent::MoveDown(count))?
                            }
                            KeyCode::Char('k') | KeyCode::Up => tx.send(AppEvent::MoveUp(count))?,
                            KeyCode::Char('G') => tx.send(AppEvent::GoBottom)?,
                            KeyCode::Char('M') => tx.send(AppEvent::GoMiddle)?,
                            KeyCode::Char('g') => app.state.key_mode = KeyMode::AwaitingG,
                            // Focus switching (l/h never play)
                            KeyCode::Char('l') | KeyCode::Right => tx.send(AppEvent::Enter)?,
                            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                                tx.send(AppEvent::Back)?
                            }

                            // Enter = play if in Explorer, else focus switch
                            KeyCode::Enter => {
                                if app.state.focus == Focus::Explorer {
                                    fire_play(&app, &spotify, &tx);
                                } else {
                                    tx.send(AppEvent::Enter)?;
                                }
                            }

                            // Space = pause/resume
                            KeyCode::Char(' ') => {
                                fire_toggle_pause(&app, &spotify, &tx);
                            }

                            KeyCode::Char('q') => tx.send(AppEvent::Quit)?,
                            _ => {}
                        }
                    }
                    KeyMode::AwaitingG => {
                        match key.code {
                            KeyCode::Char('g') => tx.send(AppEvent::GoTop)?,
                            KeyCode::Char('p') => tx.send(AppEvent::JumpToPlaylists)?,
                            KeyCode::Char('l') => tx.send(AppEvent::JumpToLiked)?,
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

// ── Play helpers (extracted so the match arm stays readable) ──────────────────

fn fire_play(app: &App, spotify: &AuthCodePkceSpotify, tx: &mpsc::UnboundedSender<AppEvent>) {
    let idx = app.state.explorer_selected_index;
    let Some(track) = app.state.explorer_items.get(idx).cloned() else {
        return;
    };
    if track.id.is_empty() {
        tracing::warn!("fire_play: track has no id, skipping");
        return;
    }

    let context_uri = match app.state.explorer_stack.last() {
        Some(ExplorerNode::PlaylistTracks(id, _, _)) => Some(format!("spotify:playlist:{id}")),
        _ => None,
    };
    let track_uri = format!("spotify:track:{}", track.id);
    let device_id = app.state.best_device_id();

    tracing::info!(
        "fire_play: track='{}' id='{}' device={:?} context={:?}",
        track.name,
        track.id,
        device_id,
        context_uri
    );

    let ctx = context_uri.clone();
    let sp = spotify.clone();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        // Step 1: ensure we have a device — fetch fresh list if needed
        let dev = match device_id {
            Some(d) => {
                tracing::info!("Using cached device: {d}");
                Some(d)
            }
            None => {
                tracing::warn!("No cached device — fetching device list");
                match svc::fetch_devices(&sp).await {
                    Ok(devs) => {
                        tracing::info!(
                            "Available devices: {:?}",
                            devs.iter().map(|d| &d.name).collect::<Vec<_>>()
                        );
                        let _ = tx2.send(AppEvent::DevicesUpdated(devs.clone()));
                        devs.into_iter().find(|d| d.is_active).map(|d| d.id)
                    }
                    Err(e) => {
                        tracing::error!("fetch_devices failed: {e:#}");
                        None
                    }
                }
            }
        };

        tracing::info!("Calling play_track with device={:?}", dev);
        match svc::play_track(&sp, &track_uri, ctx.as_deref(), dev.as_deref()).await {
            Ok(_) => {
                tracing::info!("play_track OK — polling state");
                sleep(Duration::from_millis(400)).await;
                match svc::fetch_playback_state(&sp).await {
                    Ok(ps) => {
                        tracing::info!("Post-play state: {:?}", ps.as_ref().map(|p| &p.track_name));
                        let _ = tx2.send(AppEvent::PlaybackStateUpdated(ps));
                    }
                    Err(e) => tracing::error!("post-play poll: {e:#}"),
                }
                // Refresh device list too
                if let Ok(devs) = svc::fetch_devices(&sp).await {
                    let _ = tx2.send(AppEvent::DevicesUpdated(devs));
                }
            }
            Err(e) => tracing::error!("play_track FAILED: {e:#}"),
        }
    });

    let _ = tx.send(AppEvent::PlayTrack { track, context_uri });
}

fn fire_toggle_pause(
    app: &App,
    spotify: &AuthCodePkceSpotify,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let is_playing = app
        .state
        .playback
        .as_ref()
        .map(|p| p.is_playing)
        .unwrap_or(false);
    tracing::info!("fire_toggle_pause: is_playing={is_playing}");
    let sp = spotify.clone();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let result = if is_playing {
            svc::pause(&sp).await
        } else {
            svc::resume(&sp).await
        };
        match result {
            Ok(_) => tracing::info!("toggle_pause OK"),
            Err(e) => tracing::error!("toggle_pause FAILED: {e:#}"),
        }
        sleep(Duration::from_millis(300)).await;
        if let Ok(ps) = svc::fetch_playback_state(&sp).await {
            let _ = tx2.send(AppEvent::PlaybackStateUpdated(ps));
        }
    });
    let _ = tx.send(AppEvent::TogglePause);
}

// ── Startup ───────────────────────────────────────────────────────────────────

fn spawn_initial_fetches(spotify: AuthCodePkceSpotify, tx: mpsc::UnboundedSender<AppEvent>) {
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            match svc::fetch_user(&sp).await {
                Ok(user) => {
                    let uid = user.id.clone();
                    let _ = tx.send(AppEvent::UserLoaded(user.display_name));
                    match svc::fetch_playlists(&sp, &uid).await {
                        Ok(pl) => {
                            let _ = tx.send(AppEvent::PlaylistsLoaded(pl));
                        }
                        Err(e) => {
                            tracing::error!("playlists: {e:#}");
                            let _ = tx.send(AppEvent::PlaylistsLoaded(vec![]));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("user: {e:#}");
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
                Ok(t) => {
                    let _ = tx.send(AppEvent::LikedTracksLoaded(t));
                }
                Err(e) => {
                    tracing::error!("liked: {e:#}");
                    let _ = tx.send(AppEvent::LikedTracksLoaded(vec![]));
                }
            }
        });
    }
    // Fetch devices + initial playback state on startup
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            sleep(Duration::from_millis(500)).await;
            match svc::fetch_devices(&sp).await {
                Ok(devs) => {
                    tracing::info!(
                        "Startup devices: {:?}",
                        devs.iter()
                            .map(|d| format!("{} active={}", d.name, d.is_active))
                            .collect::<Vec<_>>()
                    );
                    let _ = tx.send(AppEvent::DevicesUpdated(devs));
                }
                Err(e) => tracing::error!("fetch_devices: {e:#}"),
            }
            match svc::fetch_playback_state(&sp).await {
                Ok(ps) => {
                    let _ = tx.send(AppEvent::PlaybackStateUpdated(ps));
                }
                Err(e) => tracing::error!("initial playback state: {e:#}"),
            }
        });
    }
}

// ── Explorer fetch ────────────────────────────────────────────────────────────

fn maybe_fetch_explorer(
    current: &Option<ExplorerNode>,
    last: &mut Option<ExplorerNode>,
    in_prog: &mut bool,
    spotify: AuthCodePkceSpotify,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    if *in_prog {
        return;
    }
    let should = match (current, last.as_ref()) {
        (None, _) | (Some(ExplorerNode::LikedTracks), None) => false,
        (Some(c), None) => !matches!(c, ExplorerNode::LikedTracks),
        (Some(c), Some(p)) => !nodes_equal(c, p),
    };
    if !should {
        return;
    }
    *last = current.clone();
    *in_prog = true;
    if let Some(ExplorerNode::PlaylistTracks(id, _, _)) = current {
        let id = id.clone();
        tokio::spawn(async move {
            match svc::fetch_playlist_tracks(&spotify, &id).await {
                Ok(t) => {
                    let _ = tx.send(AppEvent::ExplorerTracksLoaded(t));
                }
                Err(e) => {
                    tracing::error!("tracks: {e:#}");
                    let _ = tx.send(AppEvent::ExplorerTracksLoaded(vec![]));
                }
            }
        });
    } else {
        *in_prog = false;
    }
}

fn nodes_equal(a: &ExplorerNode, b: &ExplorerNode) -> bool {
    match (a, b) {
        (ExplorerNode::PlaylistTracks(id1, ..), ExplorerNode::PlaylistTracks(id2, ..)) => {
            id1 == id2
        }
        (ExplorerNode::LikedTracks, ExplorerNode::LikedTracks) => true,
        _ => false,
    }
}
