//! Full-screen fuzzy search overlay.
//!
//! Activated by `/` from anywhere. Searches across ALL tracks the user has
//! (liked + every loaded playlist). Results update as you type.
//! Press Enter to play, Esc to close.

use crate::app::state::AppState;
use crate::services::spotify::TrackSummary;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

// ── Fuzzy scorer ──────────────────────────────────────────────────────────────

/// Simple but effective fuzzy score:
/// - All query chars must appear in haystack in order (subsequence match)
/// - Score = bonus for consecutive matches + bonus for early match + bonus for word-start
/// Returns None if not a match.
pub fn fuzzy_score(query: &str, haystack: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let h: Vec<char> = haystack.to_lowercase().chars().collect();

    let mut qi = 0;
    let mut score = 0i32;
    let mut last_match = 0usize;
    let mut consecutive = 0i32;

    for (hi, &hc) in h.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if hc == q[qi] {
            // Consecutive bonus
            if hi > 0 && last_match == hi.wrapping_sub(1) {
                consecutive += 1;
                score += consecutive * 4;
            } else {
                consecutive = 0;
            }
            // Word boundary bonus
            if hi == 0 || h[hi - 1] == ' ' || h[hi - 1] == '-' {
                score += 8;
            }
            // Earlier is better
            score += (100 - hi as i32).max(0);
            last_match = hi;
            qi += 1;
        }
    }

    if qi == q.len() {
        Some(score)
    } else {
        None
    }
}

/// Score a track against a query across name + artist + album
pub fn score_track(query: &str, track: &TrackSummary) -> Option<i32> {
    let name_score = fuzzy_score(query, &track.name).map(|s| s + 20);
    let artist_score = fuzzy_score(query, &track.artist);
    let album_score = fuzzy_score(query, &track.album).map(|s| s.saturating_sub(5));
    [name_score, artist_score, album_score]
        .into_iter()
        .flatten()
        .max()
}

// ── State (lives in AppState as SearchState) ──────────────────────────────────

#[derive(Default, Clone)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<TrackSummary>, // merged local + catalog, sorted by score
    pub catalog_results: Vec<TrackSummary>, // latest from Spotify /search API
    pub selected: usize,
    pub is_searching: bool, // true while Spotify API call in flight
}

impl SearchState {
    /// Rebuild results from local tracks. Call immediately on every keystroke.
    pub fn update_local(&mut self, all_tracks: &[TrackSummary]) {
        if self.query.is_empty() {
            let mut sorted = all_tracks.to_vec();
            sorted.sort_by(|a, b| a.name.cmp(&b.name));
            self.results = sorted;
            self.catalog_results.clear();
        } else {
            let q = &self.query;
            let mut scored: Vec<(i32, &TrackSummary)> = all_tracks
                .iter()
                .filter_map(|t| score_track(q, t).map(|s| (s, t)))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.results = scored.into_iter().map(|(_, t)| t.clone()).collect();
        }
        self.selected = 0;
    }

    /// Merge Spotify catalog results in, deduplicating by id, appended after local.
    pub fn merge_catalog(&mut self, catalog: Vec<TrackSummary>) {
        self.is_searching = false;
        self.catalog_results = catalog;
        // Dedup: any id already in results (local) is skipped
        let local_ids: std::collections::HashSet<String> =
            self.results.iter().map(|t| t.id.clone()).collect();
        for t in &self.catalog_results {
            if !t.id.is_empty() && !local_ids.contains(&t.id) {
                self.results.push(t.clone());
            }
        }
        // Keep selected in bounds
        if self.selected >= self.results.len() {
            self.selected = 0;
        }
    }

    pub fn selected_track(&self) -> Option<&TrackSummary> {
        self.results.get(self.selected)
    }
}

// ── Renderer ──────────────────────────────────────────────────────────────────

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = centered_rect(70, 80, frame.size());

    // Clear behind modal
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " 🔍 Search ",
            Style::default()
                .fg(Color::Rgb(137, 180, 130))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(137, 180, 130)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // query input box
            Constraint::Min(0),    // results list
            Constraint::Length(2), // hint bar
        ])
        .split(inner);

    // ── Query input ───────────────────────────────────────────────────────────
    let search = &state.search;
    let query_display = format!(" {} ", search.query);
    frame.render_widget(
        Paragraph::new(query_display)
            .style(Style::default().fg(Color::Rgb(245, 224, 220)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(88, 91, 112)))
                    .title(Span::styled(
                        " Query ",
                        Style::default().fg(Color::Rgb(150, 150, 160)),
                    )),
            ),
        layout[0],
    );

    // ── Results list ──────────────────────────────────────────────────────────
    let sel = search.selected;
    let items: Vec<ListItem> = search
        .results
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_sel = i == sel;
            let is_playing = !t.id.is_empty() && state.is_playing_track(&t.id);

            let num = if is_playing {
                Span::styled(
                    " ♫ ",
                    Style::default()
                        .fg(Color::Rgb(137, 180, 130))
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!("{:>3} ", i + 1),
                    Style::default().fg(Color::Rgb(88, 91, 112)),
                )
            };

            let name_style = if is_sel {
                Style::default()
                    .fg(Color::Rgb(245, 224, 220))
                    .add_modifier(Modifier::BOLD)
            } else if is_playing {
                Style::default().fg(Color::Rgb(137, 180, 130))
            } else {
                Style::default().fg(Color::Rgb(200, 200, 210))
            };

            let dim = if is_sel {
                Style::default().fg(Color::Rgb(180, 180, 190))
            } else {
                Style::default().fg(Color::Rgb(100, 100, 110))
            };

            let bg = if is_sel {
                Color::Rgb(40, 44, 60)
            } else {
                Color::Reset
            };

            ListItem::new(Line::from(vec![
                num,
                Span::styled(trunc(&t.name, 32), name_style.bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(trunc(&t.artist, 20), dim.bg(bg)),
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(trunc(&t.album, 18), dim.fg(Color::Rgb(80, 80, 90)).bg(bg)),
            ]))
        })
        .collect();

    let searching_suffix = if search.is_searching {
        " 🔍 searching Spotify…"
    } else {
        ""
    };
    let count_title = format!(" {} results{} ", search.results.len(), searching_suffix);
    let mut list_state = ListState::default();
    list_state.select(Some(sel));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Rgb(50, 55, 70)))
                    .title(Span::styled(
                        count_title,
                        Style::default().fg(Color::Rgb(100, 100, 110)),
                    )),
            )
            .highlight_style(Style::default().bg(Color::Rgb(40, 44, 60))),
        layout[1],
        &mut list_state,
    );

    // ── Hint bar ──────────────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Enter",
                Style::default()
                    .fg(Color::Rgb(137, 180, 130))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" play  ", Style::default().fg(Color::Rgb(100, 100, 110))),
            Span::styled(
                "↑↓",
                Style::default()
                    .fg(Color::Rgb(137, 180, 130))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " navigate  ",
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(Color::Rgb(137, 180, 130))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" close", Style::default().fg(Color::Rgb(100, 100, 110))),
        ])),
        layout[2],
    );
}

// ── Layout helper ─────────────────────────────────────────────────────────────

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_w = r.width * percent_x / 100;
    let popup_h = r.height * percent_y / 100;
    Rect {
        x: r.x + (r.width.saturating_sub(popup_w)) / 2,
        y: r.y + (r.height.saturating_sub(popup_h)) / 2,
        width: popup_w,
        height: popup_h,
    }
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
