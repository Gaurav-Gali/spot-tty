use crate::app::state::{AppState, ExplorerNode, Focus};
use crate::ui::cover::CoverArt;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

// Small per-row cover (left of table)
const ROW_COVER_W: u16 = 4;
const ROW_COVER_H: u16 = 2;

// Large detail panel cover (right side)
const DETAIL_W: u16 = 18; // terminal columns for the whole detail panel
const DETAIL_COVER_W: u16 = 16;
const DETAIL_COVER_H: u16 = 8; // 8 rows × 2 = 16 pixel rows — good square-ish shape

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = state.focus == Focus::Explorer;
    let border_style = if is_active {
        Style::default().fg(Color::Rgb(137, 180, 130))
    } else {
        Style::default().fg(Color::Rgb(88, 91, 112))
    };

    let block = Block::default()
        .title(" Explorer ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match state.explorer_stack.last() {
        None => {
            frame.render_widget(
                Paragraph::new("  Select an item from the sidebar")
                    .style(Style::default().fg(Color::Rgb(100, 100, 110))),
                inner,
            );
        }
        Some(ExplorerNode::PlaylistTracks(_, _, false)) => {
            frame.render_widget(
                Paragraph::new("  Track listing unavailable (not your playlist)")
                    .style(Style::default().fg(Color::Rgb(180, 100, 100))),
                inner,
            );
        }
        Some(ExplorerNode::PlaylistTracks(_, _, true)) | Some(ExplorerNode::LikedTracks) => {
            if state.explorer_items.is_empty() {
                frame.render_widget(
                    Paragraph::new("  Loading…")
                        .style(Style::default().fg(Color::Rgb(100, 100, 110))),
                    inner,
                );
            } else {
                render_split(frame, inner, state, is_active);
            }
        }
    }
}

fn render_split(frame: &mut Frame, area: Rect, state: &AppState, is_active: bool) {
    // Horizontal split: [row covers | track table | detail panel]
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(ROW_COVER_W + 1), // small per-row covers
            Constraint::Min(0),                  // track table
            Constraint::Length(DETAIL_W),        // detail panel
        ])
        .split(area);

    let cover_col = layout[0];
    let table_area = layout[1];
    let detail_area = layout[2];

    render_table(frame, table_area, state, is_active);
    render_row_covers(frame, cover_col, state);
    render_detail_panel(frame, detail_area, state);
}

fn render_table(frame: &mut Frame, area: Rect, state: &AppState, is_active: bool) {
    let hdr_style = Style::default()
        .fg(Color::Rgb(137, 180, 130))
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(" #"),
        Cell::from("Title"),
        Cell::from("Artist"),
        Cell::from("Album"),
        Cell::from("Time"),
    ])
    .style(hdr_style)
    .height(1);

    let fixed: u16 = 5 + 22 + 22 + 5 + 4;
    let title_w = area.width.saturating_sub(fixed).max(10);
    let widths = [
        Constraint::Length(5),
        Constraint::Length(title_w),
        Constraint::Length(22),
        Constraint::Length(22),
        Constraint::Length(5),
    ];

    let sel = state.explorer_selected_index;
    let items = &state.explorer_items;

    let rows: Vec<Row> = items
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let rel = (i as isize - sel as isize).unsigned_abs();
            let is_sel = i == sel;

            let row_style = if is_sel && is_active {
                Style::default()
                    .bg(Color::Rgb(60, 65, 80))
                    .fg(Color::Rgb(245, 224, 220))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(200, 200, 210))
            };
            let num_str = format!("{rel:>4} ");

            Row::new(vec![
                Cell::from(num_str).style(Style::default().fg(Color::Rgb(88, 91, 112))),
                Cell::from(trunc(&t.name, title_w as usize)),
                Cell::from(trunc(&t.artist, 22)),
                Cell::from(trunc(&t.album, 22)),
                Cell::from(fmt_ms(t.duration_ms)),
            ])
            .style(row_style)
            .height(ROW_COVER_H)
        })
        .collect();

    let mut ts = TableState::default();
    ts.select(Some(sel.min(items.len().saturating_sub(1))));

    frame.render_stateful_widget(
        Table::new(rows, widths).header(header).column_spacing(1),
        area,
        &mut ts,
    );
}

fn render_row_covers(frame: &mut Frame, cover_col: Rect, state: &AppState) {
    // We need to know the scroll offset — replicate the same logic as the table
    let sel = state.explorer_selected_index;
    let items = &state.explorer_items;
    let visible_rows = (cover_col.height.saturating_sub(1) / ROW_COVER_H) as usize; // -1 for header
    let scroll = sel.saturating_sub(visible_rows.saturating_sub(1));
    let header_h = 1u16;

    for (slot, track) in items.iter().enumerate().skip(scroll) {
        let row_y = cover_col.y + header_h + (slot - scroll) as u16 * ROW_COVER_H;
        if row_y + ROW_COVER_H > cover_col.y + cover_col.height {
            break;
        }

        let rect = Rect {
            x: cover_col.x,
            y: row_y,
            width: ROW_COVER_W,
            height: ROW_COVER_H,
        };
        match track
            .album_image_url
            .as_ref()
            .and_then(|u| state.cover_cache_small.get(u))
        {
            Some(art) => art.render(frame, rect),
            None => CoverArt::placeholder(ROW_COVER_W, ROW_COVER_H).render(frame, rect),
        }
    }
}

fn render_detail_panel(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::Rgb(60, 65, 80)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sel = state.explorer_selected_index;
    let Some(track) = state.explorer_items.get(sel) else {
        return;
    };

    // Vertical layout inside detail: large cover | track info
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(DETAIL_COVER_H), Constraint::Min(0)])
        .split(inner);

    // ── Large cover ───────────────────────────────────────────────────────────
    let cover_rect = Rect {
        x: inner.x + (inner.width.saturating_sub(DETAIL_COVER_W)) / 2,
        y: rows[0].y,
        width: DETAIL_COVER_W.min(inner.width),
        height: DETAIL_COVER_H,
    };

    match track
        .album_image_url
        .as_ref()
        .and_then(|u| state.cover_cache_large.get(u))
    {
        Some(art) => art.render(frame, cover_rect),
        None => {
            CoverArt::placeholder(cover_rect.width, cover_rect.height).render(frame, cover_rect)
        }
    }

    // ── Track metadata ────────────────────────────────────────────────────────
    let info_area = rows[1];
    if info_area.height == 0 {
        return;
    }

    let title_style = Style::default()
        .fg(Color::Rgb(245, 224, 220))
        .add_modifier(Modifier::BOLD);
    let artist_style = Style::default().fg(Color::Rgb(137, 180, 130));
    let label_style = Style::default().fg(Color::Rgb(88, 91, 112));
    let value_style = Style::default().fg(Color::Rgb(160, 160, 170));

    let lines: Vec<ratatui::text::Line> = vec![
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(vec![ratatui::text::Span::styled(
            trunc(&track.name, (inner.width) as usize),
            title_style,
        )]),
        ratatui::text::Line::from(vec![ratatui::text::Span::styled(
            trunc(&track.artist, inner.width as usize),
            artist_style,
        )]),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("Album  ", label_style),
            ratatui::text::Span::styled(
                trunc(&track.album, inner.width.saturating_sub(7) as usize),
                value_style,
            ),
        ]),
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("Time   ", label_style),
            ratatui::text::Span::styled(fmt_ms(track.duration_ms), value_style),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
        info_area,
    );
}

fn fmt_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn trunc(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}
