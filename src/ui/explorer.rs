use crate::app::state::{AppState, ExplorerNode, Focus};
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

    // ── Build item list from state ────────────────────────────────────────
    let raw_items: Vec<String> = match state.explorer_stack.last() {
        Some(ExplorerNode::PlaylistTracks(_, _, _)) | Some(ExplorerNode::LikedTracks) => {
            if state.explorer_items.is_empty() {
                // Distinguish between "still loading" and "not owner so tracks unavailable"
                let not_owner = matches!(
                    state.explorer_stack.last(),
                    Some(ExplorerNode::PlaylistTracks(_, _, false))
                );
                if not_owner {
                    vec!["Track listing unavailable (not your playlist)".to_string()]
                } else {
                    vec!["Loading…".to_string()]
                }
            } else {
                state
                    .explorer_items
                    .iter()
                    .map(|t| format!("{} — {}", t.name, t.artist))
                    .collect()
            }
        }
        Some(ExplorerNode::ArtistAlbums(_, _)) => {
            if state.explorer_albums.is_empty() {
                vec!["Loading…".to_string()]
            } else {
                state.explorer_albums.clone()
            }
        }
        None => vec!["Select an item from the sidebar".to_string()],
    };

    let selected = state.explorer_selected_index;

    // ── Build rows with correct relative line numbers ─────────────────────
    // The relative number for item at index `i` is |i - selected|.
    // We build ALL items (not just visible height) and let ratatui handle
    // scrolling via ListState — this way the highlight and numbers are always
    // consistent regardless of scroll position.
    let rows: Vec<ListItem> = raw_items
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let rel = (i as isize - selected as isize).unsigned_abs();
            ListItem::new(format!("{:>3} │ {}", rel, text))
        })
        .collect();

    let mut list_state = ListState::default();
    if !raw_items.is_empty() {
        // Always track selection position so ratatui knows where to scroll —
        // even when the panel is inactive, so it stays in the right position.
        list_state.select(Some(selected.min(raw_items.len().saturating_sub(1))));
    }

    let list = List::new(rows).block(block).highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut list_state);
}
