//! Track action menu — opened with `i` on a selected track in the Explorer.
//!
//! Shows a fuzzy-filterable list of actions:
//!   ♥  Like / Unlike
//!   +  Add to queue
//!   ↗  Add to playlist  (expands into a sub-list of playlists)
//!   ▶  Play now
//!   ✕  Cancel
//!
//! Typing narrows the action list. Enter confirms. Esc closes.

use crate::app::state::AppState;
use crate::services::spotify::TrackSummary;
use crate::ui::search::centered_rect;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

// ── Action definitions ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrackAction {
    PlayNow,
    AddToQueue,
}

impl TrackAction {
    pub fn label(&self) -> String {
        match self {
            Self::PlayNow => "▶  Play now".to_string(),
            Self::AddToQueue => "+  Add to queue".to_string(),
        }
    }
}

// ── Menu state ────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct TrackMenuState {
    /// The track this menu is open for
    pub track: Option<TrackSummary>,
    /// Typed filter query
    pub query: String,
    /// Filtered action list
    pub actions: Vec<TrackAction>,
    pub selected: usize,
}

impl TrackMenuState {
    pub fn open(track: TrackSummary) -> Self {
        let mut s = Self {
            track: Some(track),
            query: String::new(),
            actions: vec![],
            selected: 0,
        };
        s.rebuild_actions();
        s
    }

    pub fn rebuild_actions(&mut self) {
        let mut all: Vec<TrackAction> = vec![TrackAction::PlayNow, TrackAction::AddToQueue];

        if self.query.is_empty() {
            self.actions = all;
        } else {
            let q = self.query.to_lowercase();
            self.actions = all
                .into_iter()
                .filter(|a| a.label().to_lowercase().contains(&q))
                .collect();
        }
        self.selected = self.selected.min(self.actions.len().saturating_sub(1));
    }

    pub fn selected_action(&self) -> Option<&TrackAction> {
        self.actions.get(self.selected)
    }
}

// ── Renderer ──────────────────────────────────────────────────────────────────

pub fn render(frame: &mut Frame, state: &AppState) {
    let menu = &state.track_menu;
    let track = match &menu.track {
        Some(t) => t,
        None => return,
    };

    let area = centered_rect(50, 70, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            format!(" ⚙  {} ", trunc(&track.name, 30)),
            Style::default()
                .fg(Color::Rgb(245, 224, 220))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(198, 160, 246))); // mauve accent
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // track info strip
            Constraint::Length(3), // filter input
            Constraint::Min(0),    // actions list
            Constraint::Length(2), // hints
        ])
        .split(inner);

    // ── Track info strip ──────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                trunc(&track.artist, 24),
                Style::default().fg(Color::Rgb(137, 180, 130)),
            ),
            Span::styled("  —  ", Style::default().fg(Color::Rgb(60, 60, 70))),
            Span::styled(
                trunc(&track.album, 24),
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ),
        ])),
        layout[0],
    );

    // ── Filter input ──────────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(format!(" {} ", menu.query))
            .style(Style::default().fg(Color::Rgb(245, 224, 220)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(88, 91, 112)))
                    .title(Span::styled(
                        " Filter actions ",
                        Style::default().fg(Color::Rgb(100, 100, 110)),
                    )),
            ),
        layout[1],
    );

    // ── Actions list ──────────────────────────────────────────────────────────
    let sel = menu.selected;
    let items: Vec<ListItem> = menu
        .actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let is_sel = i == sel;
            let (icon_color, label_color) = match action {
                TrackAction::PlayNow => (Color::Rgb(137, 180, 130), Color::Rgb(200, 200, 210)),
                TrackAction::AddToQueue => (Color::Rgb(137, 220, 235), Color::Rgb(200, 200, 210)),
            };
            let bg = if is_sel {
                Color::Rgb(40, 40, 55)
            } else {
                Color::Reset
            };
            // Split label into owned icon + rest so there's no borrow of local `label`
            let label = action.label();
            let split = label
                .char_indices()
                .nth(3)
                .map(|(i, _)| i)
                .unwrap_or(label.len());
            let icon: String = label[..split].to_string();
            let rest: String = label[split..].to_string();
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    icon,
                    Style::default()
                        .fg(icon_color)
                        .bg(bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    rest,
                    Style::default()
                        .fg(if is_sel {
                            Color::Rgb(245, 224, 220)
                        } else {
                            label_color
                        })
                        .bg(bg),
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(sel));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(50, 55, 70))),
            )
            .highlight_style(Style::default().bg(Color::Rgb(40, 40, 55))),
        layout[2],
        &mut list_state,
    );

    // ── Hints ─────────────────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Enter",
                Style::default()
                    .fg(Color::Rgb(198, 160, 246))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm  ", Style::default().fg(Color::Rgb(100, 100, 110))),
            Span::styled(
                "↑↓",
                Style::default()
                    .fg(Color::Rgb(198, 160, 246))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " navigate  ",
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(Color::Rgb(198, 160, 246))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" close", Style::default().fg(Color::Rgb(100, 100, 110))),
        ])),
        layout[3],
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
