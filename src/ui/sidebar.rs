use crate::app::state::{AppState, Focus};
use crate::ui::cover::{render_placeholder, write_image_sentinel, RenderCache};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

const COVER_W: u16 = 8;
const COVER_H: u16 = 4;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, cache: &mut RenderCache) {
    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let name = state.user_name.as_deref().unwrap_or("Connecting…");
    frame.render_widget(
        Paragraph::new(format!(" {name} (press 'p')"))
            .style(Style::default().fg(Color::Rgb(205, 214, 244)))
            .block(
                Block::default()
                    .title(" Account ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(88, 91, 112))),
            ),
        panes[0],
    );

    let library = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(panes[1]);

    render_playlists(frame, library[0], state, cache);
    render_liked(frame, library[1], state);
}

fn render_playlists(frame: &mut Frame, area: Rect, state: &AppState, cache: &mut RenderCache) {
    let is_active = state.focus == Focus::Sidebar;

    if !state.loaded_playlists {
        frame.render_widget(
            Paragraph::new(" Loading…")
                .style(Style::default().fg(Color::Rgb(100, 100, 110)))
                .block(Block::default().title(" Playlists ").borders(Borders::ALL)),
            area,
        );
        return;
    }

    let block = Block::default().title(" Playlists ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.playlists.is_empty() {
        frame.render_widget(
            Paragraph::new(" No playlists").style(Style::default().fg(Color::Rgb(100, 100, 110))),
            inner,
        );
        return;
    }

    let sel = state.navigation.selected_index;
    let visible_rows = (inner.height / COVER_H) as usize;
    let scroll = sel.saturating_sub(visible_rows.saturating_sub(1));
    let protocol = state.image_protocol;

    for (slot, pl) in state
        .playlists
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_rows)
    {
        let row_y = inner.y + (slot as u16 * COVER_H);
        if row_y + COVER_H > inner.y + inner.height {
            break;
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(COVER_W + 1), Constraint::Min(0)])
            .split(Rect {
                x: inner.x,
                y: row_y,
                width: inner.width,
                height: COVER_H,
            });

        let cover_rect = Rect {
            x: cols[0].x,
            y: cols[0].y,
            width: COVER_W,
            height: COVER_H,
        };
        match pl.image_url.as_ref().and_then(|u| state.cover_cache.get(u)) {
            Some(img) => {
                write_image_sentinel(frame, cover_rect);
                img.render(frame, cover_rect, protocol, cache);
            }
            None => render_placeholder(frame, cover_rect),
        }

        let i = slot + scroll;
        let is_sel = i == sel;
        let rel = (i as isize - sel as isize).unsigned_abs();
        let bg = if is_sel && is_active {
            Color::Rgb(60, 65, 80)
        } else {
            Color::Reset
        };
        let name_s = if is_sel && is_active {
            Style::default()
                .fg(Color::Rgb(245, 224, 220))
                .add_modifier(Modifier::BOLD)
                .bg(bg)
        } else {
            Style::default().fg(Color::Rgb(180, 180, 190)).bg(bg)
        };
        let dim = Style::default().fg(Color::Rgb(100, 100, 110)).bg(bg);
        let num = Style::default().fg(Color::Rgb(88, 91, 112)).bg(bg);
        let text_rect = Rect {
            x: cols[1].x + 1,
            y: cols[1].y,
            width: cols[1].width.saturating_sub(1),
            height: COVER_H,
        };

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(format!("{rel:>3} │ "), num),
                    Span::styled(trunc(&pl.name, text_rect.width as usize - 6), name_s),
                ]),
                Line::from(vec![
                    Span::styled("    │ ", num),
                    Span::styled(
                        format!(
                            "{} tracks{}",
                            pl.track_count,
                            if pl.owner { "" } else { "  ⊘" }
                        ),
                        dim,
                    ),
                ]),
                Line::from(vec![Span::styled("    │", num)]),
                Line::from(vec![Span::styled("    │", num)]),
            ])
            .style(Style::default().bg(bg)),
            text_rect,
        );
    }
}

fn render_liked(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = state.focus == Focus::Sidebar;
    let pl_len = state.playlists.len();
    let selected = state.navigation.selected_index == pl_len;
    let rel = (pl_len as isize - state.navigation.selected_index as isize).unsigned_abs();
    let hl = if is_active && selected {
        Style::default()
            .bg(Color::Rgb(60, 65, 80))
            .fg(Color::Rgb(245, 224, 220))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::Rgb(35, 35, 40))
            .fg(Color::Rgb(120, 120, 130))
            .add_modifier(Modifier::DIM)
    };
    let item = ListItem::new(Line::from(vec![
        Span::styled(
            format!("{rel:>3} │ "),
            Style::default().fg(Color::Rgb(88, 91, 112)),
        ),
        Span::raw("♥  Liked Songs"),
    ]));
    let mut ls = ListState::default();
    if selected {
        ls.select(Some(0));
    }
    frame.render_stateful_widget(
        List::new(vec![item])
            .block(
                Block::default()
                    .title(" Liked Songs ")
                    .borders(Borders::ALL),
            )
            .highlight_style(hl),
        area,
        &mut ls,
    );
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

pub fn render_no_images(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut dummy = RenderCache::default();
    render(frame, area, state, &mut dummy);
}
