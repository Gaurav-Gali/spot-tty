use crate::app::state::{AppState, ExplorerNode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // ─────────────────────────────────────────────
    // Title / breadcrumb
    // ─────────────────────────────────────────────
    let breadcrumb = match state.explorer_stack.last() {
        Some(ExplorerNode::PlaylistTracks(_, name, _)) => {
            format!("Library › Playlist › {}", name)
        }
        Some(ExplorerNode::ArtistAlbums(_, name)) => {
            format!("Library › Artist › {}", name)
        }
        Some(ExplorerNode::LikedTracks) => "Library › Liked Songs".to_string(),
        None => "Library".to_string(),
    };

    let current_track = "Track Name";
    let title = Line::from(vec![
        Span::raw(breadcrumb),
        Span::raw(" › "),
        Span::styled(
            current_track,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    // ─────────────────────────────────────────────
    // Padding
    // ─────────────────────────────────────────────
    let padded = Rect {
        x: inner.x + 2,
        y: inner.y,
        width: inner.width.saturating_sub(4),
        height: inner.height,
    };

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(14),
            Constraint::Min(10),
            Constraint::Length(4),
            Constraint::Length(16),
        ])
        .split(padded);

    let playhead_area = layout[0];
    let sweep_area = layout[1];
    let visualizer_area = layout[3];

    // ─────────────────────────────────────────────
    // Progress bar background
    // ─────────────────────────────────────────────
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(20, 35, 20))),
        sweep_area,
    );

    // ─────────────────────────────────────────────
    // Filled progress
    // ─────────────────────────────────────────────
    let progress = state.playback_progress.clamp(0.0, 1.0);
    let fill_width = (sweep_area.width as f64 * progress) as u16;
    if fill_width > 0 {
        let progress_rect = Rect {
            x: sweep_area.x,
            y: sweep_area.y,
            width: fill_width,
            height: sweep_area.height,
        };
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Green)),
            progress_rect,
        );
    }

    // ─────────────────────────────────────────────
    // Playhead text
    // ─────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new("▶ 1:23 / 3:45").style(Style::default().fg(Color::White)),
        playhead_area,
    );

    // ─────────────────────────────────────────────
    // Animated visualizer
    // ─────────────────────────────────────────────
    let bars = ["▂", "▅", "▇", "▆", "▃", "▂", "▇", "▅", "▃", "▂"];
    let mut visual = String::new();
    for i in 0..10 {
        let index = (state.visualizer_phase + i) % bars.len();
        visual.push_str(bars[index]);
    }
    frame.render_widget(
        Paragraph::new(visual).style(Style::default().fg(Color::Green)),
        visualizer_area,
    );
}
