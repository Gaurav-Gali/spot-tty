use crate::app::state::{AppState, ExplorerNode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

// Number of visualizer bars — keep narrow
const N_BARS: usize = 10;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {

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
            Constraint::Length(15),            // "▶ 1:23 / 3:45"
            Constraint::Min(10),               // progress bar
            Constraint::Length(2),             // gap
            Constraint::Length(N_BARS as u16), // visualizer
        ])
        .split(padded);

    let playhead_area = layout[0];
    let sweep_area = layout[1];
    let visualizer_area = layout[3];


    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(20, 35, 20))),
        sweep_area,
    );
    let fill_w = (sweep_area.width as f64 * state.playback_progress().clamp(0.0, 1.0)) as u16;
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


    let is_playing = state
        .playback
        .as_ref()
        .map(|p| p.is_playing)
        .unwrap_or(false);
    render_visualizer(frame, visualizer_area, state.visualizer_phase, is_playing);
}


//
// Each bar is driven by a smooth sine wave with a unique frequency and phase.
// Because we write one cell per bar directly into ratatui's buffer, each bar
// can have its own colour that depends on its current height — green at the
// bottom, yellow in the middle, coral/rose at the top.

fn render_visualizer(frame: &mut Frame, area: Rect, phase: usize, is_playing: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Max bar height = area height (1 cell = one block character)
    let _max_h = area.height as f64;

    // Compute heights for each bar using smooth sine waves
    let bar_heights: [f64; N_BARS] = {
        let mut hs = [0f64; N_BARS];
        let t = phase as f64 * 0.08; // slow, smooth time progression

        // Each bar: sum of two sine waves with different frequencies
        // Frequencies chosen so bars drift apart gracefully (not coprime chaos)
        let freqs: [(f64, f64); N_BARS] = [
            (1.0, 2.3),
            (1.3, 1.7),
            (0.7, 3.1),
            (1.6, 2.0),
            (0.9, 2.7),
            (1.2, 1.5),
            (0.8, 3.3),
            (1.5, 2.1),
            (1.1, 1.9),
            (0.6, 2.5),
        ];
        let phase_offsets: [f64; N_BARS] = [0.0, 0.7, 1.4, 2.1, 2.8, 3.5, 4.2, 4.9, 5.6, 6.3];
        for i in 0..N_BARS {
            if is_playing {
                let (f1, f2) = freqs[i];
                let phi = phase_offsets[i];
                // Two layered sines → organic, never perfectly flat or spiky
                let raw = 0.55 * (t * f1 + phi).sin()
                    + 0.30 * (t * f2 + phi * 1.3).sin()
                    + 0.15 * (t * 3.7 + phi * 0.5).sin();
                // Normalise from [-1, 1] to [0.05, 1.0] — always at least a sliver
                hs[i] = (raw + 1.0) * 0.5 * 0.95 + 0.05;
            } else {
                // Paused: gentle decay to a very low flat line
                hs[i] = 0.06;
            }
        }
        hs
    };

    // Map height fraction → colour gradient
    // 0.0–0.4  →  green   (137, 220, 130)
    // 0.4–0.7  →  teal→yellow  interpolated
    // 0.7–1.0  →  amber→coral  (240, 140, 80)
    fn bar_color(h: f64) -> Color {
        let h = h.clamp(0.0, 1.0);
        if h < 0.45 {
            // Green zone: interpolate from dark green to bright green
            let t = h / 0.45;
            Color::Rgb(
                lerp(60, 137, t) as u8,
                lerp(160, 220, t) as u8,
                lerp(80, 130, t) as u8,
            )
        } else if h < 0.72 {
            // Yellow zone: green → yellow
            let t = (h - 0.45) / 0.27;
            Color::Rgb(
                lerp(137, 250, t) as u8,
                lerp(220, 210, t) as u8,
                lerp(130, 60, t) as u8,
            )
        } else {
            // Coral/rose zone: yellow → coral
            let t = (h - 0.72) / 0.28;
            Color::Rgb(
                lerp(250, 235, t) as u8,
                lerp(210, 90, t) as u8,
                lerp(60, 80, t) as u8,
            )
        }
    }

    // Block characters — 8 levels
    let glyphs = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

    let buf = frame.buffer_mut();
    for (i, &h) in bar_heights.iter().enumerate() {
        let col = area.x + i as u16;
        if col >= area.x + area.width {
            break;
        }

        // We have exactly one cell row — encode height as block character
        let glyph_idx = ((h * glyphs.len() as f64) as usize).clamp(0, glyphs.len() - 1);
        let glyph = glyphs[glyph_idx];
        let color = bar_color(h);

        // Write into the single cell row
        let cell = buf.get_mut(col, area.y);
        cell.set_symbol(glyph);
        cell.set_fg(color);
        cell.set_bg(Color::Rgb(15, 15, 20)); // near-black bg so colours pop
    }
}

fn lerp(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b - a) as f64 * t.clamp(0.0, 1.0)) as i32
}

fn fmt_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}
