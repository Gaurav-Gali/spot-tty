use crate::app::state::{AppState, ExplorerNode, Focus};
use crate::ui::cover::{render_placeholder, RenderCache};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

const ROW_COVER_W: u16 = 4;
const ROW_COVER_H: u16 = 2;
const DETAIL_PANEL_W: u16 = 32;
const DETAIL_COVER_W: u16 = 30;
const DETAIL_COVER_H: u16 = 15;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, cache: &mut RenderCache) {
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
                render_split(frame, inner, state, is_active, cache);
            }
        }
    }
}

fn render_split(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    is_active: bool,
    cache: &mut RenderCache,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(ROW_COVER_W + 1),
            Constraint::Min(0),
            Constraint::Length(DETAIL_PANEL_W),
        ])
        .split(area);

    render_table(frame, layout[1], state, is_active);
    render_row_covers(frame, layout[0], state, cache);
    render_detail(frame, layout[2], state, cache);
}

fn render_table(frame: &mut Frame, area: Rect, state: &AppState, is_active: bool) {
    let hdr = Row::new(vec![
        Cell::from(" #"),
        Cell::from("Title"),
        Cell::from("Artist"),
        Cell::from("Album"),
        Cell::from("Time"),
    ])
    .style(
        Style::default()
            .fg(Color::Rgb(137, 180, 130))
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    let fixed: u16 = 5 + 22 + 22 + 5 + 4;
    let title_w: u16 = area.width.saturating_sub(fixed).max(10);
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
            let style = if is_sel && is_active {
                Style::default()
                    .bg(Color::Rgb(60, 65, 80))
                    .fg(Color::Rgb(245, 224, 220))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(200, 200, 210))
            };
            Row::new(vec![
                Cell::from(format!("{rel:>4} "))
                    .style(Style::default().fg(Color::Rgb(88, 91, 112))),
                Cell::from(trunc(&t.name, title_w as usize)),
                Cell::from(trunc(&t.artist, 22)),
                Cell::from(trunc(&t.album, 22)),
                Cell::from(fmt_ms(t.duration_ms)),
            ])
            .style(style)
            .height(ROW_COVER_H)
        })
        .collect();

    let mut ts = TableState::default();
    ts.select(Some(sel.min(items.len().saturating_sub(1))));
    frame.render_stateful_widget(
        Table::new(rows, widths).header(hdr).column_spacing(1),
        area,
        &mut ts,
    );
}

fn render_row_covers(frame: &mut Frame, area: Rect, state: &AppState, cache: &mut RenderCache) {
    let sel = state.explorer_selected_index;
    let vis = (area.height.saturating_sub(1) / ROW_COVER_H) as usize;
    let scroll = sel.saturating_sub(vis.saturating_sub(1));
    let protocol = state.image_protocol;

    for (slot, track) in state
        .explorer_items
        .iter()
        .enumerate()
        .skip(scroll)
        .take(vis + 1)
    {
        let row_y = area.y + 1 + slot as u16 * ROW_COVER_H;
        if row_y + ROW_COVER_H > area.y + area.height {
            break;
        }
        let rect = Rect {
            x: area.x,
            y: row_y,
            width: ROW_COVER_W,
            height: ROW_COVER_H,
        };
        match track
            .album_image_url
            .as_ref()
            .and_then(|u| state.cover_cache.get(u))
        {
            Some(img) => img.render(frame, rect, protocol, cache),
            None => render_placeholder(frame, rect),
        }
    }
}

fn render_detail(frame: &mut Frame, area: Rect, state: &AppState, cache: &mut RenderCache) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::Rgb(50, 55, 70)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sel = state.explorer_selected_index;
    let Some(track) = state.explorer_items.get(sel) else {
        return;
    };
    let protocol = state.image_protocol;

    let cover_h = DETAIL_COVER_H.min(inner.height);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(cover_h), Constraint::Min(0)])
        .split(inner);

    let cw = DETAIL_COVER_W.min(inner.width);
    let cx = inner.x + (inner.width.saturating_sub(cw)) / 2;
    let cover_rect = Rect {
        x: cx,
        y: rows[0].y,
        width: cw,
        height: cover_h,
    };

    match track
        .album_image_url
        .as_ref()
        .and_then(|u| state.cover_cache.get(u))
    {
        Some(img) => img.render(frame, cover_rect, protocol, cache),
        None => render_placeholder(frame, cover_rect),
    }

    let meta = rows[1];
    if meta.height == 0 {
        return;
    }
    let w = inner.width as usize;
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            trunc(&track.name, w),
            Style::default()
                .fg(Color::Rgb(245, 224, 220))
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            trunc(&track.artist, w),
            Style::default().fg(Color::Rgb(137, 180, 130)),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Album  ", Style::default().fg(Color::Rgb(88, 91, 112))),
            Span::styled(
                trunc(&track.album, w.saturating_sub(7)),
                Style::default().fg(Color::Rgb(160, 160, 170)),
            ),
        ]),
        Line::from(vec![
            Span::styled("Time   ", Style::default().fg(Color::Rgb(88, 91, 112))),
            Span::styled(
                fmt_ms(track.duration_ms),
                Style::default().fg(Color::Rgb(160, 160, 170)),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), meta);
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
