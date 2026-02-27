use crate::app::state::{AppState, AppStatus, Focus};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

// ─────────────────────────────────────────────────────────────────────────────
// Highlight styles
// ─────────────────────────────────────────────────────────────────────────────

fn active_highlight() -> Style {
    Style::default()
        .bg(Color::Rgb(60, 65, 80))
        .fg(Color::Rgb(245, 224, 220))
        .add_modifier(Modifier::BOLD)
}

fn inactive_highlight() -> Style {
    Style::default()
        .bg(Color::Rgb(35, 35, 40))
        .fg(Color::Rgb(120, 120, 130))
        .add_modifier(Modifier::DIM)
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    // Outer split: user box (fixed) + library sections (remaining)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_user_box(frame, outer[0], state);

    // If still loading, show a single spinner box instead of empty lists
    if state.status == AppStatus::Loading {
        render_loading(frame, outer[1]);
        return;
    }

    // The three library sections share the remaining height
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ])
        .split(outer[1]);

    let pl_len = state.playlists.len();

    render_section(
        frame,
        sections[0],
        " Playlists ",
        &state
            .playlists
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        state,
        0,
    );

    render_section(
        frame,
        sections[1],
        " Liked Songs ",
        &["Liked Songs"],
        state,
        pl_len,
    );

    render_section(
        frame,
        sections[2],
        " Artists ",
        &state
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        state,
        pl_len + 1,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-widgets
// ─────────────────────────────────────────────────────────────────────────────

fn render_user_box(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let name = state.user_name.as_deref().unwrap_or("Connecting…");

    let paragraph = Paragraph::new(format!(" {}", name))
        .style(Style::default().fg(Color::Rgb(205, 214, 244)))
        .block(
            Block::default()
                .title(" Account ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(88, 91, 112))),
        );

    frame.render_widget(paragraph, area);
}

fn render_loading(frame: &mut Frame, area: ratatui::layout::Rect) {
    let paragraph = Paragraph::new(" Loading library…")
        .style(Style::default().fg(Color::Rgb(120, 120, 130)))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_section(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    title: &str,
    items: &[&str],
    state: &AppState,
    offset: usize,
) {
    let is_active = state.focus == Focus::Sidebar;
    let highlight = if is_active {
        active_highlight()
    } else {
        inactive_highlight()
    };

    let height = area.height as usize;
    let mut rows: Vec<ListItem> = Vec::new();

    for row in 0..height {
        if row < items.len() {
            let absolute_index = offset + row;
            let number =
                (absolute_index as isize - state.navigation.selected_index as isize).abs() as usize;
            rows.push(ListItem::new(format!("{:>3} │ {}", number, items[row])));
        } else {
            rows.push(ListItem::new("    │"));
        }
    }

    let mut list_state = ListState::default();
    if state.navigation.selected_index >= offset
        && state.navigation.selected_index < offset + items.len()
    {
        list_state.select(Some(state.navigation.selected_index - offset));
    }

    let list = List::new(rows)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(highlight);

    frame.render_stateful_widget(list, area, &mut list_state);
}
