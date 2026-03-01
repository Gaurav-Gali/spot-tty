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
use ui::trackmenu::TrackAction;

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
    let mut last_search_query = String::new();
    let mut last_search_fire = Instant::now();
    let search_debounce = Duration::from_millis(400);
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
                app.state.visualizer_phase = (app.state.visualizer_phase + 1) % 100_000;
                if let Some(p) = &mut app.state.playback {
                    p.progress_ms = (p.progress_ms + 150).min(p.duration_ms);
                }
            }
        }

        // ── Poll playback every 2 s ───────────────────────────────────────────
        if last_poll.elapsed() >= poll_rate {
            last_poll = Instant::now();
            let sp = spotify.clone();
            let tx2 = tx.clone();
            tokio::spawn(async move {
                if let Ok(s) = svc::fetch_playback_state(&sp).await {
                    let _ = tx2.send(AppEvent::PlaybackStateUpdated(s));
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
        let overlay_open = matches!(
            app.state.key_mode,
            KeyMode::Search | KeyMode::TrackMenu | KeyMode::Profile
        );

        app.state.render_cache.begin_frame();

        // When an overlay opens, wipe all Kitty images so they don't bleed
        // through the modal. Images will re-upload cleanly when overlay closes.
        if overlay_open {
            app.state.render_cache.clear_kitty_images();
        }

        let cache_ptr = &mut app.state.render_cache as *mut _;
        terminal.draw(|f| {
            let cache = unsafe { &mut *cache_ptr };
            let areas = ui::layout::split(f.size());
            // Only render cover images when no overlay is open
            if !overlay_open {
                ui::sidebar::render(f, areas.sidebar, &app.state, cache);
                ui::explorer::render(f, areas.main, &app.state, cache);
            } else {
                // Still render text/borders, just skip image queuing
                ui::sidebar::render_no_images(f, areas.sidebar, &app.state);
                ui::explorer::render_no_images(f, areas.main, &app.state);
            }
            ui::player::render(f, areas.control, &app.state);
            // Overlays on top
            if app.state.key_mode == KeyMode::Search {
                ui::search::render(f, &app.state);
            }
            if app.state.key_mode == KeyMode::TrackMenu {
                ui::trackmenu::render(f, &app.state);
            }
            if app.state.key_mode == KeyMode::Profile {
                ui::profile::render(f, &app.state);
            }
            // Toast
            if let Some(msg) = app.state.active_toast() {
                render_toast(f, msg);
            }
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

        // ── Debounced Spotify catalog search ─────────────────────────────────
        if app.state.key_mode == KeyMode::Search {
            let q = app.state.search.query.clone();
            if q != last_search_query {
                last_search_query = q.clone();
                last_search_fire = Instant::now();
            } else if !q.is_empty()
                && app.state.search.is_searching
                && last_search_fire.elapsed() >= search_debounce
            {
                // Enough time has passed — fire the Spotify API search
                last_search_fire = Instant::now() + Duration::from_secs(9999); // prevent re-fire
                let sp = spotify.clone();
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let results = svc::search_tracks(&sp, &q, 30).await.unwrap_or_default();
                    let _ = tx2.send(AppEvent::SearchCatalogResults(results));
                });
            }
        } else {
            last_search_query.clear();
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
                match app.state.key_mode {
                    // ── Search overlay input ───────────────────────────────────
                    KeyMode::Search => {
                        match key.code {
                            KeyCode::Esc => {
                                tx.send(AppEvent::CloseSearch)?;
                            }
                            KeyCode::Enter => {
                                if let Some(track) = app.state.search.selected_track().cloned() {
                                    tx.send(AppEvent::CloseSearch)?;
                                    fire_play_track(&track, None, &app, &spotify, &tx);
                                }
                            }
                            // Arrow keys only for list navigation — j/k type into query
                            KeyCode::Up => {
                                tx.send(AppEvent::MoveUp(1))?;
                            }
                            KeyCode::Down => {
                                tx.send(AppEvent::MoveDown(1))?;
                            }
                            KeyCode::Backspace => {
                                let mut q = app.state.search.query.clone();
                                q.pop();
                                tx.send(AppEvent::SearchQueryChanged(q))?;
                            }
                            KeyCode::Char(c) => {
                                let mut q = app.state.search.query.clone();
                                q.push(c);
                                tx.send(AppEvent::SearchQueryChanged(q))?;
                            }
                            _ => {}
                        }
                    }

                    // ── Track menu overlay input ───────────────────────────────
                    KeyMode::TrackMenu => {
                        match key.code {
                            KeyCode::Esc => {
                                tx.send(AppEvent::CloseTrackMenu)?;
                            }
                            // Arrow keys navigate actions; j/k type into filter
                            KeyCode::Up => {
                                tx.send(AppEvent::MoveUp(1))?;
                            }
                            KeyCode::Down => {
                                tx.send(AppEvent::MoveDown(1))?;
                            }
                            KeyCode::Enter => {
                                if let Some(action) =
                                    app.state.track_menu.selected_action().cloned()
                                {
                                    if let Some(track) = app.state.track_menu.track.clone() {
                                        tx.send(AppEvent::CloseTrackMenu)?;
                                        fire_track_action(action, track, &app, &spotify, &tx);
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                let mut q = app.state.track_menu.query.clone();
                                q.pop();
                                tx.send(AppEvent::TrackMenuQueryChanged(q))?;
                            }
                            KeyCode::Char(c) => {
                                let mut q = app.state.track_menu.query.clone();
                                q.push(c);
                                tx.send(AppEvent::TrackMenuQueryChanged(q))?;
                            }
                            _ => {}
                        }
                    }

                    // ── Profile overlay input ──────────────────────────────────────
                    KeyMode::Profile => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('p') => {
                                tx.send(AppEvent::CloseProfile)?;
                            }
                            // j/k switch sections; clear logout focus first
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.state.profile.logout_sel = false;
                                tx.send(AppEvent::MoveUp(1))?;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.state.profile.logout_sel = false;
                                tx.send(AppEvent::MoveDown(1))?;
                            }
                            // Tab toggles logout button focus when in Profile section
                            KeyCode::Tab | KeyCode::BackTab => {
                                if app.state.profile.section == ui::profile::ProfileSection::Profile
                                {
                                    app.state.profile.logout_sel = !app.state.profile.logout_sel;
                                }
                            }
                            KeyCode::Enter => {
                                if app.state.profile.logout_sel {
                                    // confirmed — fire logout
                                    tx.send(AppEvent::ProfileLogout)?;
                                } else if app.state.profile.section
                                    == ui::profile::ProfileSection::Profile
                                {
                                    // first Enter → focus the logout button
                                    app.state.profile.logout_sel = true;
                                }
                            }
                            _ => {}
                        }
                    }

                    // ── Normal + AwaitingG ─────────────────────────────────────
                    KeyMode::Normal | KeyMode::AwaitingG => {
                        // Digits go to prefix counter (only in Normal mode)
                        if app.state.key_mode == KeyMode::Normal {
                            if let KeyCode::Char(c) = key.code {
                                if c.is_ascii_digit() {
                                    let d = c.to_digit(10).unwrap() as usize;
                                    app.state.pending_count =
                                        Some(app.state.pending_count.unwrap_or(0) * 10 + d);
                                    continue;
                                }
                            }
                        }
                        let count = app.state.pending_count.take().unwrap_or(1);

                        match app.state.key_mode {
                            KeyMode::Normal => match key.code {
                                // Motion
                                KeyCode::Char('j') | KeyCode::Down => {
                                    tx.send(AppEvent::MoveDown(count))?
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    tx.send(AppEvent::MoveUp(count))?
                                }
                                KeyCode::Char('G') => tx.send(AppEvent::GoBottom)?,
                                KeyCode::Char('M') => tx.send(AppEvent::GoMiddle)?,
                                KeyCode::Char('g') => app.state.key_mode = KeyMode::AwaitingG,
                                // Focus
                                KeyCode::Char('l') | KeyCode::Right => tx.send(AppEvent::Enter)?,
                                KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                                    tx.send(AppEvent::Back)?
                                }
                                // Play on Enter
                                KeyCode::Enter => {
                                    if app.state.focus == Focus::Explorer {
                                        fire_play(&app, &spotify, &tx);
                                    } else {
                                        tx.send(AppEvent::Enter)?;
                                    }
                                }
                                // Space = pause/resume
                                KeyCode::Char(' ') => fire_toggle_pause(&app, &spotify, &tx),
                                // Search overlay
                                KeyCode::Char('/') => {
                                    tx.send(AppEvent::OpenSearch)?;
                                }
                                // Track menu (only in Explorer)
                                KeyCode::Char('p') => {
                                    tx.send(AppEvent::OpenProfile)?;
                                    let sp = spotify.clone();
                                    let tx2 = tx.clone();
                                    tokio::spawn(async move {
                                        if let Ok(profile) = svc::fetch_user(&sp).await {
                                            let _ = tx2.send(AppEvent::UserProfileLoaded(profile));
                                        }
                                    });
                                }
                                KeyCode::Char('i') => {
                                    if app.state.focus == Focus::Explorer
                                        && !app.state.explorer_items.is_empty()
                                    {
                                        tx.send(AppEvent::OpenTrackMenu)?;
                                        // Async: check if track is liked
                                        if let Some(track) = app
                                            .state
                                            .explorer_items
                                            .get(app.state.explorer_selected_index)
                                            .cloned()
                                        {
                                            let sp = spotify.clone();
                                            let tx2 = tx.clone();
                                            tokio::spawn(async move {
                                                if let Ok(liked) =
                                                    svc::is_track_liked(&sp, &track.id).await
                                                {
                                                    let _ = tx2.send(
                                                        AppEvent::TrackMenuLikedStatus(liked),
                                                    );
                                                }
                                            });
                                        }
                                    }
                                }
                                KeyCode::Char('q') => tx.send(AppEvent::Quit)?,
                                _ => {}
                            },
                            KeyMode::AwaitingG => {
                                match key.code {
                                    KeyCode::Char('g') => tx.send(AppEvent::GoTop)?,
                                    KeyCode::Char('p') => tx.send(AppEvent::JumpToPlaylists)?,
                                    KeyCode::Char('l') => tx.send(AppEvent::JumpToLiked)?,
                                    _ => {}
                                }
                                app.state.key_mode = KeyMode::Normal;
                            }
                            _ => {}
                        }
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

// ── Toast renderer ─────────────────────────────────────────────────────────────

fn render_toast(f: &mut ratatui::Frame, msg: &str) {
    use ratatui::{
        layout::Rect,
        style::{Color, Style},
        widgets::{Block, Borders, Clear, Paragraph},
    };
    let area = f.size();
    let w = (msg.len() as u16 + 6).min(area.width);
    let h = 3u16;
    let toast_area = Rect {
        x: area.width.saturating_sub(w + 1),
        y: area.height.saturating_sub(h + 1),
        width: w,
        height: h,
    };
    f.render_widget(Clear, toast_area);
    f.render_widget(
        Paragraph::new(format!(" {} ", msg))
            .style(
                Style::default()
                    .fg(Color::Rgb(245, 224, 220))
                    .bg(Color::Rgb(40, 44, 60)),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(137, 180, 130))),
            ),
        toast_area,
    );
}

// ── Play helpers ──────────────────────────────────────────────────────────────

fn fire_play(app: &App, spotify: &AuthCodePkceSpotify, tx: &mpsc::UnboundedSender<AppEvent>) {
    let idx = app.state.explorer_selected_index;
    if let Some(track) = app.state.explorer_items.get(idx).cloned() {
        let context_uri = match app.state.explorer_stack.last() {
            Some(ExplorerNode::PlaylistTracks(id, _, _)) => Some(format!("spotify:playlist:{id}")),
            _ => None,
        };
        fire_play_track(&track, context_uri, app, spotify, tx);
    }
}

fn fire_play_track(
    track: &svc::TrackSummary,
    context_uri: Option<String>,
    app: &App,
    spotify: &AuthCodePkceSpotify,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    if track.id.is_empty() {
        return;
    }
    let track_uri = format!("spotify:track:{}", track.id);
    let device_id = app.state.best_device_id();
    let ctx = context_uri.clone();
    let sp = spotify.clone();
    let tx2 = tx.clone();
    tracing::info!("play_track: '{}' device={:?}", track.name, device_id);
    tokio::spawn(async move {
        let dev = match device_id {
            Some(d) => Some(d),
            None => match svc::fetch_devices(&sp).await {
                Ok(devs) => {
                    let _ = tx2.send(AppEvent::DevicesUpdated(devs.clone()));
                    devs.into_iter().find(|d| d.is_active).map(|d| d.id)
                }
                Err(e) => {
                    tracing::error!("fetch_devices: {e:#}");
                    None
                }
            },
        };
        match svc::play_track(&sp, &track_uri, ctx.as_deref(), dev.as_deref()).await {
            Ok(_) => {
                sleep(Duration::from_millis(400)).await;
                if let Ok(ps) = svc::fetch_playback_state(&sp).await {
                    let _ = tx2.send(AppEvent::PlaybackStateUpdated(ps));
                }
            }
            Err(e) => {
                tracing::error!("play_track: {e:#}");
                let _ = tx2.send(AppEvent::Toast(format!("Play failed: {e}")));
            }
        }
    });
    let _ = tx.send(AppEvent::PlayTrack {
        track: track.clone(),
        context_uri,
    });
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
    let sp = spotify.clone();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let r = if is_playing {
            svc::pause(&sp).await
        } else {
            svc::resume(&sp).await
        };
        if let Err(e) = r {
            tracing::error!("toggle_pause: {e:#}");
        }
        sleep(Duration::from_millis(300)).await;
        if let Ok(ps) = svc::fetch_playback_state(&sp).await {
            let _ = tx2.send(AppEvent::PlaybackStateUpdated(ps));
        }
    });
    let _ = tx.send(AppEvent::TogglePause);
}

fn fire_track_action(
    action: TrackAction,
    track: svc::TrackSummary,
    app: &App,
    spotify: &AuthCodePkceSpotify,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let sp = spotify.clone();
    let tx2 = tx.clone();
    match action {
        TrackAction::PlayNow => {
            let context_uri = match app.state.explorer_stack.last() {
                Some(ExplorerNode::PlaylistTracks(id, _, _)) => {
                    Some(format!("spotify:playlist:{id}"))
                }
                _ => None,
            };
            fire_play_track(&track, context_uri, app, spotify, tx);
        }
        TrackAction::AddToQueue => {
            let name = track.name.clone();
            tokio::spawn(async move {
                match svc::add_to_queue(&sp, &track.id).await {
                    Ok(_) => {
                        let _ = tx2.send(AppEvent::Toast(format!("Added to queue: {name}")));
                    }
                    Err(e) => {
                        tracing::error!("add_to_queue: {e:#}");
                        let _ = tx2.send(AppEvent::Toast(format!("Failed: {e}")));
                    }
                }
            });
        }
        TrackAction::Like => {
            let name = track.name.clone();
            tokio::spawn(async move {
                match svc::like_track(&sp, &track.id).await {
                    Ok(_) => {
                        let _ = tx2.send(AppEvent::Toast(format!("♥ Liked: {name}")));
                    }
                    Err(e) => {
                        tracing::error!("like_track: {e:#}");
                        let _ = tx2.send(AppEvent::Toast(format!("Failed: {e}")));
                    }
                }
            });
        }
        TrackAction::Unlike => {
            let name = track.name.clone();
            tokio::spawn(async move {
                match svc::unlike_track(&sp, &track.id).await {
                    Ok(_) => {
                        let _ = tx2.send(AppEvent::Toast(format!("♡ Unliked: {name}")));
                    }
                    Err(e) => {
                        tracing::error!("unlike_track: {e:#}");
                        let _ = tx2.send(AppEvent::Toast(format!("Failed: {e}")));
                    }
                }
            });
        }
        TrackAction::AddToPlaylist(playlist_id, playlist_name) => {
            let name = track.name.clone();
            tokio::spawn(async move {
                match svc::add_to_playlist(&sp, &playlist_id, &track.id).await {
                    Ok(_) => {
                        let _ = tx2.send(AppEvent::Toast(format!("Added to {playlist_name}")));
                    }
                    Err(e) => {
                        tracing::error!("add_to_playlist: {e:#}");
                        let _ = tx2.send(AppEvent::Toast(format!("Failed: {e}")));
                    }
                }
            });
        }
    }
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
    {
        let (sp, tx) = (spotify.clone(), tx.clone());
        tokio::spawn(async move {
            sleep(Duration::from_millis(500)).await;
            if let Ok(devs) = svc::fetch_devices(&sp).await {
                tracing::info!(
                    "devices: {:?}",
                    devs.iter().map(|d| &d.name).collect::<Vec<_>>()
                );
                let _ = tx.send(AppEvent::DevicesUpdated(devs));
            }
            if let Ok(ps) = svc::fetch_playback_state(&sp).await {
                let _ = tx.send(AppEvent::PlaybackStateUpdated(ps));
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
