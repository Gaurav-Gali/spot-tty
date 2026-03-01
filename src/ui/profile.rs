//! Profile overlay.

use crate::app::state::AppState;
use crate::ui::search::centered_rect;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};



#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ProfileSection {
    #[default]
    Profile,
    Stats,
    Commands,
}

impl ProfileSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Stats => "Stats",
            Self::Commands => "Commands",
        }
    }
    pub fn all() -> [Self; 3] {
        [Self::Profile, Self::Stats, Self::Commands]
    }
    pub fn index(self) -> usize {
        match self {
            Self::Profile => 0,
            Self::Stats => 1,
            Self::Commands => 2,
        }
    }
}

#[derive(Default, Clone)]
pub struct ProfileState {
    pub section: ProfileSection,
    pub logout_sel: bool,
}

impl ProfileState {
    pub fn next_section(&mut self) {
        self.section = match self.section {
            ProfileSection::Profile => ProfileSection::Stats,
            ProfileSection::Stats => ProfileSection::Commands,
            ProfileSection::Commands => ProfileSection::Profile,
        };
        self.logout_sel = false;
    }
    pub fn prev_section(&mut self) {
        self.section = match self.section {
            ProfileSection::Profile => ProfileSection::Commands,
            ProfileSection::Stats => ProfileSection::Profile,
            ProfileSection::Commands => ProfileSection::Stats,
        };
        self.logout_sel = false;
    }
}


const ACCENT: Color = Color::Rgb(137, 180, 130);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const PEACH: Color = Color::Rgb(235, 160, 100);
const RED: Color = Color::Rgb(243, 139, 168);
const FG: Color = Color::Rgb(205, 214, 244);
const SUBTEXT: Color = Color::Rgb(108, 112, 134);
const OVERLAY: Color = Color::Rgb(49, 50, 68);
const SEL_BG: Color = Color::Rgb(40, 44, 60);
const BASE: Color = Color::Reset; // matches app background



pub fn render(frame: &mut Frame, state: &AppState) {
    let area = centered_rect(82, 88, frame.size());
    frame.render_widget(Clear, area);

    // Fill whole modal with dark base colour first so nothing bleeds through
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let outer = Block::default()
        .title(Span::styled(
            " ⚙  Profile ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BASE));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    // Divider
    let div_col = cols[1];
    for row in 0..div_col.height {
        let cell = frame.buffer_mut().get_mut(div_col.x, div_col.y + row);
        cell.set_symbol("│");
        cell.set_fg(ACCENT);
        cell.set_bg(BASE);
    }

    render_left_nav(frame, cols[0], state);

    match state.profile.section {
        ProfileSection::Profile => render_profile(frame, cols[2], state),
        ProfileSection::Stats => render_stats(frame, cols[2], state),
        ProfileSection::Commands => render_commands(frame, cols[2], state),
    }
}



fn render_left_nav(frame: &mut Frame, area: Rect, state: &AppState) {
    // Fill background
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let sections = ProfileSection::all();
    let items: Vec<ListItem> = sections
        .iter()
        .map(|&s| {
            let is_sel = s == state.profile.section;
            ListItem::new(Line::from(vec![
                Span::raw(if is_sel { " ▶ " } else { "   " }),
                Span::styled(
                    s.label(),
                    if is_sel {
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(SUBTEXT)
                    },
                ),
            ]))
            .style(Style::default().bg(if is_sel { SEL_BG } else { BASE }))
        })
        .collect();

    let mut ls = ListState::default();
    ls.select(Some(state.profile.section.index()));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_stateful_widget(
        List::new(items).highlight_style(Style::default().bg(SEL_BG)),
        rows[1],
        &mut ls,
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(" j/k  navigate", Style::default().fg(SUBTEXT))),
            Line::from(Span::styled(" Esc  close", Style::default().fg(SUBTEXT))),
        ])
        .style(Style::default().bg(BASE)),
        rows[3],
    );
}



fn render_profile(frame: &mut Frame, area: Rect, state: &AppState) {
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let content = pad(rows[0], 3, 2);

    if let Some(p) = &state.user_profile {

        let initial = p
            .display_name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .next()
            .unwrap_or('?');
        let tier_badge = match p.product.as_deref() {
            Some("premium") => Span::styled(
                " ✦ Premium ",
                Style::default()
                    .fg(Color::Rgb(30, 30, 40))
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            _ => Span::styled(
                " Free ",
                Style::default().fg(Color::Rgb(30, 30, 40)).bg(SUBTEXT),
            ),
        };

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(
                    format!(" {} ", initial),
                    Style::default()
                        .fg(Color::Rgb(20, 20, 30))
                        .bg(ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    p.display_name.clone(),
                    Style::default().fg(FG).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                tier_badge,
            ]),
            Line::from(""),
            // Separator
            Line::from(Span::styled(
                "─".repeat(content.width.saturating_sub(2) as usize),
                Style::default().fg(OVERLAY),
            )),
            Line::from(""),
        ];


        lines.push(fline("ID", p.id.clone()));
        if let Some(e) = &p.email {
            lines.push(fline("Email", e.clone()));
        }
        if let Some(c) = &p.country {
            lines.push(fline("Country", c.clone()));
        }
        lines.push(fline("Followers", p.followers.to_string()));
        lines.push(Line::from(""));


        lines.push(fstat("Playlists", state.playlists.len().to_string()));
        lines.push(fstat("Liked songs", state.liked_tracks.len().to_string()));

        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(BASE)),
            content,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled("  Loading…", Style::default().fg(SUBTEXT)))
                .style(Style::default().bg(BASE)),
            content,
        );
    }


    let is_sel = state.profile.logout_sel;
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if is_sel { RED } else { OVERLAY }))
            .title(Span::styled(
                if is_sel {
                    " ⏻  Log out — Enter to confirm "
                } else {
                    " ⏻  Log out "
                },
                Style::default()
                    .fg(if is_sel { RED } else { SUBTEXT })
                    .add_modifier(if is_sel {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ))
            .style(Style::default().bg(BASE)),
        rows[1],
    );

    // Hint for reaching logout
    if !is_sel {
        let hint_area = Rect {
            x: rows[1].x + 2,
            y: rows[1].y + 1,
            width: rows[1].width.saturating_sub(4),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                "press Tab to select, Enter to confirm",
                Style::default().fg(SUBTEXT),
            ))
            .style(Style::default().bg(BASE)),
            hint_area,
        );
    }
}



fn render_stats(frame: &mut Frame, area: Rect, state: &AppState) {
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let stats = &state.cached_stats;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(48),
            Constraint::Length(2),
            Constraint::Percentage(50),
        ])
        .split(pad(area, 2, 1));

    let left = cols[0];
    let right = cols[2];


    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(0)])
        .split(left);

    // Numbers card
    let total_h = format_duration(stats.total_duration_ms);
    let avg_ms = if stats.total_liked > 0 {
        stats.total_duration_ms / stats.total_liked as u64
    } else {
        0
    };
    let avg_s = avg_ms / 1000;
    let avg_str = format!("{}:{:02}", avg_s / 60, avg_s % 60);

    let num_lines = vec![
        Line::from(""),
        stat_line("Liked tracks", stats.total_liked.to_string(), ACCENT),
        stat_line("Playlists", stats.total_playlists.to_string(), ACCENT),
        stat_line("Owned", stats.owned_playlists.to_string(), ACCENT),
        stat_line("Artists", stats.unique_artists.to_string(), MAUVE),
        stat_line("Albums", stats.unique_albums.to_string(), MAUVE),
        stat_line("Total time", total_h, PEACH),
        stat_line("Avg track", avg_str, PEACH),
        Line::from(""),
    ];
    frame.render_widget(
        Paragraph::new(num_lines)
            .style(Style::default().bg(BASE))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(OVERLAY))
                    .style(Style::default().bg(BASE))
                    .title(Span::styled(
                        " Library ",
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    )),
            ),
        left_rows[0],
    );

    // Top Artists — simple list with short inline bar
    render_ranked_list(
        frame,
        left_rows[1],
        &stats.top_artists,
        "Top Artists",
        ACCENT,
    );


    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(right);

    render_time_card(frame, right_rows[0], &stats);
    render_ranked_list(frame, right_rows[1], &stats.top_albums, "Top Albums", MAUVE);
}

/// Simple ranked list with a proportional bar that fits within its own column.
/// No overflow — bar width is always calculated from the available content width.
fn render_ranked_list(frame: &mut Frame, area: Rect, items: &[String], title: &str, color: Color) {
    if area.height < 3 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(OVERLAY))
        .style(Style::default().bg(BASE))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    let content = block.inner(area);
    frame.render_widget(block, area);

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  Browse playlists to populate",
                Style::default().fg(SUBTEXT),
            ))
            .style(Style::default().bg(BASE)),
            content,
        );
        return;
    }

    // Reserve space: "  N. " prefix (5 chars) + bar (fixed 8) + " " + label (rest)
    let prefix_w = 5usize;
    let bar_w = 8usize;
    let gap = 1usize;
    let label_w = (content.width as usize).saturating_sub(prefix_w + bar_w + gap + 1);
    let n = items.len(); // used to scale bars

    let lines: Vec<Line> = items
        .iter()
        .take(content.height as usize)
        .enumerate()
        .map(|(i, name)| {
            // Bar shrinks linearly from full to 1 char
            let filled = bar_w.saturating_sub(i * bar_w / n.max(1)).max(1);
            let empty = bar_w - filled;

            // Colour: brightest at top, dims toward bottom
            let t = 1.0 - (i as f64 / n as f64) * 0.6;
            let col = dim_color(color, t);

            Line::from(vec![
                Span::styled(format!("  {:>2}. ", i + 1), Style::default().fg(SUBTEXT)),
                Span::styled("█".repeat(filled), Style::default().fg(col)),
                Span::styled("░".repeat(empty), Style::default().fg(OVERLAY)),
                Span::raw(" "),
                Span::styled(trunc(name, label_w), Style::default().fg(FG)),
            ])
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        content,
    );
}

fn render_time_card(frame: &mut Frame, area: Rect, stats: &crate::services::spotify::UserStats) {
    let days = stats.total_duration_ms / 86_400_000;
    let hours = (stats.total_duration_ms % 86_400_000) / 3_600_000;
    let mins = (stats.total_duration_ms % 3_600_000) / 60_000;

    // Proportional bar across full content width (computed dynamically)
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(OVERLAY))
        .style(Style::default().bg(BASE))
        .title(Span::styled(
            " Listening Time ",
            Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
        ));
    let content = block.inner(area);
    frame.render_widget(block, area);

    let bar_w = content.width.saturating_sub(2) as usize;
    let total = stats.total_duration_ms.max(1) as f64;
    let d_cells = ((days as f64 * 86_400_000.0 / total) * bar_w as f64) as usize;
    let h_cells = (((stats.total_duration_ms % 86_400_000) as f64 / total) * bar_w as f64) as usize;
    let m_cells = bar_w.saturating_sub(d_cells + h_cells);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("█".repeat(d_cells.min(bar_w)), Style::default().fg(ACCENT)),
            Span::styled(
                "▓".repeat(h_cells.min(bar_w.saturating_sub(d_cells))),
                Style::default().fg(MAUVE),
            ),
            Span::styled("░".repeat(m_cells), Style::default().fg(PEACH)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("█", Style::default().fg(ACCENT)),
            Span::styled(format!(" {}d  ", days), Style::default().fg(FG)),
            Span::styled("▓", Style::default().fg(MAUVE)),
            Span::styled(format!(" {}h  ", hours), Style::default().fg(FG)),
            Span::styled("░", Style::default().fg(PEACH)),
            Span::styled(format!(" {}m", mins), Style::default().fg(FG)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  that's ", Style::default().fg(SUBTEXT)),
            Span::styled(
                format!("{days}"),
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" full days of music", Style::default().fg(SUBTEXT)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        content,
    );
}



fn render_commands(frame: &mut Frame, area: Rect, _state: &AppState) {
    frame.render_widget(Block::default().style(Style::default().bg(BASE)), area);

    let padded = pad(area, 2, 1);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(padded);

    let nav_cmds: &[(&str, &str)] = &[
        ("j / ↓", "Move cursor down"),
        ("k / ↑", "Move cursor up"),
        ("l / → / Enter", "Focus explorer"),
        ("h / ← / Bksp", "Focus sidebar"),
        ("G", "Jump to bottom"),
        ("M", "Jump to middle"),
        ("gg", "Jump to top"),
        ("gp", "Jump to Playlists"),
        ("gl", "Jump to Liked Songs"),
        ("1..9", "Numeric prefix (e.g. 5j)"),
    ];
    let play_cmds: &[(&str, &str)] = &[
        ("Enter", "Play selected track"),
        ("Space", "Pause / Resume"),
        ("n", "Next track"),
        ("N", "Previous track"),
        ("/", "Search (fuzzy)"),
        ("i", "Track info & actions"),
        ("p", "Profile & stats"),
        ("q", "Quit"),
    ];
    let action_cmds: &[(&str, &str)] = &[
        ("Enter (search)", "Play result"),
        ("Enter (info)", "Confirm action"),
        ("↑ ↓  (search)", "Navigate results"),
        ("Esc", "Close overlay"),
        ("i → ♥", "Like / Unlike"),
        ("i → +", "Add to queue"),
        ("i → ↗", "Add to playlist"),
    ];

    render_cmd_block(frame, cols[0], "Navigation", nav_cmds, ACCENT);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(play_cmds.len() as u16 + 2),
            Constraint::Min(0),
        ])
        .split(cols[1]);
    render_cmd_block(frame, right[0], "Playback & Overlays", play_cmds, MAUVE);
    render_cmd_block(frame, right[1], "Overlay Actions", action_cmds, PEACH);
}

fn render_cmd_block(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    cmds: &[(&str, &str)],
    color: Color,
) {
    let lines: Vec<Line> = cmds
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!(" {:18}", key),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Style::default().fg(FG)),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(BASE))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(OVERLAY))
                    .style(Style::default().bg(BASE))
                    .title(Span::styled(
                        format!(" {title} "),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    )),
            ),
        area,
    );
}



/// Label + value field, left-aligned, consistent label width
fn fline(label: &'static str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}  ",), Style::default().fg(SUBTEXT)),
        Span::styled(value.into(), Style::default().fg(FG)),
    ])
}

/// Inline stat: label on left, bold value on right
fn fstat(label: &'static str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), Style::default().fg(SUBTEXT)),
        Span::styled(
            value.into(),
            Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Stats line with coloured value
fn stat_line(label: &'static str, value: impl Into<String>, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<16}"), Style::default().fg(SUBTEXT)),
        Span::styled(
            value.into(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Dim a colour toward dark by factor t (1.0 = full, 0.4 = quite dim)
fn dim_color(c: Color, t: f64) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((r as f64 * t).clamp(0.0, 255.0)) as u8,
            ((g as f64 * t).clamp(0.0, 255.0)) as u8,
            ((b as f64 * t).clamp(0.0, 255.0)) as u8,
        ),
        other => other,
    }
}

fn format_duration(ms: u64) -> String {
    let s = ms / 1000;
    format!(
        "{}d {}h {}m",
        s / 86400,
        (s % 86400) / 3600,
        (s % 3600) / 60
    )
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

fn pad(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x + x,
        y: area.y + y,
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
    }
}
