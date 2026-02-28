use crate::app::state::{AppState, ExplorerNode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // ── Breadcrumb + now playing track name ───────────────────────────────────
    let breadcrumb = match state.explorer_stack.last() {
        Some(ExplorerNode::PlaylistTracks(_, name, _)) => format!("Library › {} ", name),
        Some(ExplorerNode::LikedTracks) => "Library › Liked Songs ".to_string(),
        None => "Library ".to_string(),
    };

    let title_line = if let Some(p) = &state.playback {
        let icon = if p.is_playing { "▶" } else { "⏸" };
        Line::from(vec![
            Span::styled(
                format!(" {breadcrumb}› "),
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ),
            Span::styled(icon, Style::default().fg(Color::Rgb(137, 180, 130))),
            Span::raw(" "),
            Span::styled(
                format!("{} — {}", p.track_name, p.artist),
                Style::default()
                    .fg(Color::Rgb(245, 224, 220))
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {breadcrumb}"),
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ),
            Span::styled(
                "No active playback",
                Style::default().fg(Color::Rgb(88, 91, 112)),
            ),
        ])
    };

    let block = Block::default().borders(Borders::ALL).title(title_line);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let padded = Rect {
        x: inner.x + 2,
        y: inner.y,
        width: inner.width.saturating_sub(4),
        height: inner.height,
    };

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(14), // playhead "▶ 1:23 / 3:45"
            Constraint::Min(10),    // progress bar
            Constraint::Length(4),  // gap
            Constraint::Length(16), // visualizer
        ])
        .split(padded);

    let playhead_area = layout[0];
    let sweep_area = layout[1];
    let visualizer_area = layout[3];

    // ── Progress bar ──────────────────────────────────────────────────────────
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(20, 35, 20))),
        sweep_area,
    );

    let progress = state.playback_progress().clamp(0.0, 1.0);
    let fill_w = (sweep_area.width as f64 * progress) as u16;
    if fill_w > 0 {
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Rgb(137, 180, 130))),
            Rect {
                x: sweep_area.x,
                y: sweep_area.y,
                width: fill_w,
                height: sweep_area.height,
            },
        );
    }

    // ── Playhead text ─────────────────────────────────────────────────────────
    let playhead_str = match &state.playback {
        Some(p) => {
            let icon = if p.is_playing { "▶" } else { "⏸" };
            format!(
                "{} {} / {}",
                icon,
                fmt_ms(p.progress_ms),
                fmt_ms(p.duration_ms)
            )
        }
        None => "  --:-- / --:--".to_string(),
    };
    frame.render_widget(
        Paragraph::new(playhead_str).style(Style::default().fg(Color::Rgb(200, 200, 210))),
        playhead_area,
    );

    // ── Visualizer — only animates when playing ────────────────────────────────
    let bars = ["▂", "▅", "▇", "▆", "▃", "▂", "▇", "▅", "▃", "▂"];
    let visual = if state
        .playback
        .as_ref()
        .map(|p| p.is_playing)
        .unwrap_or(false)
    {
        let mut s = String::new();
        for i in 0..10 {
            s.push_str(bars[(state.visualizer_phase + i) % bars.len()]);
        }
        s
    } else {
        "▂▂▂▂▂▂▂▂▂▂".to_string() // flat when paused
    };
    frame.render_widget(
        Paragraph::new(visual).style(Style::default().fg(Color::Rgb(137, 180, 130))),
        visualizer_area,
    );
}

fn fmt_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}
