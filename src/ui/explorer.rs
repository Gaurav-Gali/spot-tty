use crate::app::state::{AppState, AppStatus, ExplorerNode, Focus};
use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

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

pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let is_active = state.focus == Focus::Explorer;
    let highlight = if is_active {
        active_highlight()
    } else {
        inactive_highlight()
    };

    let border_style = if is_active {
        Style::default().fg(Color::Rgb(137, 180, 130))
    } else {
        Style::default().fg(Color::Rgb(88, 91, 112))
    };

    let block = Block::default()
        .title(" Explorer ")
        .borders(Borders::ALL)
        .border_style(border_style);

    // ── Build item rows from real state data ──────────────────────────────
    let raw_items: Vec<String> = match state.explorer_stack.last() {
        Some(ExplorerNode::PlaylistTracks(_, _)) | Some(ExplorerNode::LikedTracks) => {
            if state.explorer_items.is_empty() && state.status == AppStatus::Loading {
                vec!["Loading tracks…".to_string()]
            } else {
                state
                    .explorer_items
                    .iter()
                    .map(|t| format!("{} — {}", t.name, t.artist))
                    .collect()
            }
        }
        Some(ExplorerNode::ArtistAlbums(_, _)) => {
            if state.explorer_albums.is_empty() && state.status == AppStatus::Loading {
                vec!["Loading albums…".to_string()]
            } else {
                state.explorer_albums.clone()
            }
        }
        None => vec!["Select an item from the sidebar".to_string()],
    };

    let height = area.height as usize;
    let mut rows: Vec<ListItem> = Vec::new();

    for row in 0..height {
        if row < raw_items.len() {
            let number = (row as isize - state.explorer_selected_index as isize).abs() as usize;
            rows.push(ListItem::new(format!("{:>3} │ {}", number, raw_items[row])));
        } else {
            rows.push(ListItem::new("    │"));
        }
    }

    let mut list_state = ListState::default();
    if is_active && !raw_items.is_empty() {
        list_state.select(Some(state.explorer_selected_index));
    }

    let list = List::new(rows).block(block).highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut list_state);
}
