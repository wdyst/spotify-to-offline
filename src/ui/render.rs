use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};

use super::{App, PlaylistStatus, Screen};

// ── Palette ───────────────────────────────────────────────────────────────────

const ACCENT:   Color = Color::Cyan;
const DIM:      Color = Color::DarkGray;
const OK:       Color = Color::Green;
const WARN:     Color = Color::Yellow;
const ERR:      Color = Color::Red;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw_main(f: &mut Frame, app: &App) {
    let area = f.area();

    // Reserve one line at the bottom for the hint bar
    let [main, hint_row] = vsplit(area, [Constraint::Min(0), Constraint::Length(1)]);
    let [left, right]    = hsplit(main, [28, 72]);
    let [log_area, gauge_area] = vsplit(right, [Constraint::Min(3), Constraint::Length(3)]);

    draw_playlist_panel(f, app, left);
    draw_log_panel(f, app, log_area);
    draw_gauge(f, app, gauge_area);
    draw_hint_bar(f, hint_row, app.screen, app.running, app.filter_mode);

    // Import input overlay
    if app.screen == Screen::Import {
        draw_import_bar(f, area, app);
    }

    // Delete confirmation popup
    if let Some(name) = &app.confirm_delete {
        draw_confirm_popup(f, area, name);
    }

    // Help popup (drawn last so it's on top of everything)
    if app.show_help {
        draw_help_popup(f, area);
    }
}

// ── Playlist panel ────────────────────────────────────────────────────────────

fn draw_playlist_panel(f: &mut Frame, app: &App, area: Rect) {
    if app.playlist_names.is_empty() {
        draw_home_menu(f, app, area);
        return;
    }

    // Title changes when filter mode is active
    let vis_count = app.visible_playlist_names().len();
    let title_str = if app.filter_mode {
        format!(" / {}█ ({}) ", app.playlist_filter, vis_count)
    } else if !app.playlist_filter.is_empty() {
        format!(" / {} ({}) — Esc clears ", app.playlist_filter, vis_count)
    } else if app.running && app.screen == Screen::Working {
        " Downloading… ".to_string()
    } else {
        " Playlists ".to_string()
    };

    let border_col = if app.filter_mode || !app.playlist_filter.is_empty() {
        WARN
    } else {
        ACCENT
    };

    let block = Block::default()
        .title(title_str)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_col));

    // Compute the filtered list of names to display
    let vis_names: Vec<&str> = {
        if app.playlist_filter.is_empty() {
            app.playlist_names.iter().map(|s| s.as_str()).collect()
        } else {
            let f = app.playlist_filter.to_lowercase();
            app.playlist_names.iter()
                .filter(|n| n.to_lowercase().contains(&f))
                .map(|s| s.as_str())
                .collect()
        }
    };

    let total   = vis_names.len();
    let visible = area.height.saturating_sub(2) as usize;
    let sel     = app.playlist_sel;
    let start   = sel
        .saturating_sub(visible.saturating_sub(1))
        .min(total.saturating_sub(visible));

    let items: Vec<ListItem> = vis_names
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, name)| {
            let is_sel  = i == sel;
            let is_dl   = app.dl_progress.contains_key(*name) && app.running;
            let prefix  = if is_dl { "⬇ " } else if is_sel { "▶ " } else { "  " };

            // Colour by disk status; selection overrides to ACCENT
            let color = if is_sel {
                ACCENT
            } else {
                match app.playlist_statuses.get(*name) {
                    Some(PlaylistStatus::Complete) => OK,
                    Some(PlaylistStatus::Partial)  => WARN,
                    Some(PlaylistStatus::Empty)    => ERR,
                    _                              => Color::White,
                }
            };
            let modifier = if is_sel || is_dl {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };

            // Counts: live progress while downloading, else scanned disk state
            let count_str = if let (true, Some(p)) = (app.running, app.dl_progress.get(*name)) {
                format!(" {}/{}", p.done, p.total)
            } else {
                match app.playlist_counts.get(*name) {
                    Some((done, total)) if *total > 0 => format!(" {}/{}", done, total),
                    _ => String::new(),
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}{}", prefix, name),
                    Style::default().fg(color).add_modifier(modifier),
                ),
                Span::styled(count_str, Style::default().fg(DIM)),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Shown when there are no playlists imported yet — acts as a home menu.
fn draw_home_menu(f: &mut Frame, app: &App, area: Rect) {
    let status_line = if app.running {
        Span::styled(" Working… ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" Ready ", Style::default().fg(DIM))
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" spotify-to-offline ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]))
        .title_bottom(Line::from(vec![status_line]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    let menu = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  No playlists imported yet.", Style::default().fg(DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [i]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  Import Exportify ZIP"),
        ]),
        Line::from(vec![
            Span::styled("  [m]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  Generate M3U files"),
        ]),
        Line::from(vec![
            Span::styled("  [s]", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  Settings"),
        ]),
        Line::from(vec![
            Span::styled("  [q]", Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
            Span::styled("  Quit", Style::default().fg(DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tip: ", Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
            Span::styled(
                "press [i] and hit Enter",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  to auto-detect your CSVs",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ),
        ]),
    ];

    let para = Paragraph::new(menu).block(block);
    f.render_widget(para, area);
}

// ── Log panel ─────────────────────────────────────────────────────────────────

const VINYL: [&str; 8] = ["♩ ", "♩♪", "♪♫", "♫♬", "♬♩", "♩♫", "♪♬", "♫♩"];

fn draw_log_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ♫ ", Style::default().fg(ACCENT)),
            Span::styled("Log", Style::default().fg(DIM)),
            Span::styled(" ♫ ", Style::default().fg(ACCENT)),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    // Reserve the first row for a permanent ASCII header; logs use the rest.
    let inner_h  = area.height.saturating_sub(2) as usize;
    let log_rows = inner_h.saturating_sub(1);
    let total    = app.logs.len();
    let start    = if total > log_rows {
        app.log_scroll.min(total - log_rows)
    } else {
        0
    };

    // Static header line — optionally shows a vinyl animation while downloading
    let mut header_spans = vec![
        Span::styled(" ♩ ♪ ", Style::default().fg(ACCENT)),
        Span::styled(
            "spotify-to-offline",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ♫ ♬ ", Style::default().fg(ACCENT)),
    ];
    if app.running {
        let frame = VINYL[app.anim_frame as usize % VINYL.len()];
        header_spans.push(Span::styled(
            format!("  ─  {} ", frame),
            Style::default().fg(ACCENT),
        ));
    }
    let header = Line::from(header_spans);

    let mut lines: Vec<Line> = vec![header];
    lines.extend(app.logs
        .iter()
        .skip(start)
        .take(log_rows)
        .map(|entry| Line::from(vec![
            Span::styled(format!("[{}] ", entry.ts), Style::default().fg(DIM)),
            Span::styled(entry.text.clone(), log_style(&entry.text)),
        ])));

    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}

fn log_style(text: &str) -> Style {
    if text.contains("━━") {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else if text.contains('✓') || text.contains('✔') || text.contains("ok [") {
        Style::default().fg(OK)
    } else if text.contains('⚠') || text.contains("quality_warn") || text.contains("warn") {
        Style::default().fg(WARN)
    } else if text.contains('✗') || text.contains("failed") || text.contains("error") {
        Style::default().fg(ERR)
    } else if text.contains('▶') || text.contains('⬇') || text.contains('♪') {
        Style::default().fg(ACCENT)
    } else if text.contains("not found") {
        Style::default().fg(DIM)
    } else {
        Style::default().fg(Color::White)
    }
}

// ── Progress gauge ────────────────────────────────────────────────────────────

fn draw_gauge(f: &mut Frame, app: &App, area: Rect) {
    let label = if app.progress_label.is_empty() {
        "Idle".to_string()
    } else if app.progress_pct == 100 {
        format!("{} — done", app.progress_label)
    } else if app.running {
        format!("{}  {}%", app.progress_label, app.progress_pct)
    } else {
        app.progress_label.clone()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    let gauge = Gauge::default()
        .block(block)
        .gauge_style(Style::default().fg(ACCENT))
        .percent(app.progress_pct)
        .label(label);

    f.render_widget(gauge, area);
}

// ── Import input overlay ──────────────────────────────────────────────────────

fn draw_import_bar(f: &mut Frame, area: Rect, app: &App) {
    // Float a small box near the bottom of the screen
    let height = 3u16;
    let width  = area.width.saturating_sub(4);
    let y      = area.height.saturating_sub(height + 2);
    let x      = 2u16;

    let popup = Rect { x, y, width, height };

    // Clear behind the popup
    f.render_widget(Clear, popup);

    let prompt_text = format!(
        "{}█",
        if app.import_buf.is_empty() {
            // Show placeholder when empty
            String::new()
        } else {
            app.import_buf.clone()
        }
    );

    let content = if app.import_buf.is_empty() {
        vec![Line::from(vec![
            Span::styled("blank = auto-detect  ", Style::default().fg(DIM)),
            Span::styled("█", Style::default().fg(ACCENT).add_modifier(Modifier::SLOW_BLINK)),
        ])]
    } else {
        vec![Line::from(vec![
            Span::styled(&app.import_buf, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(ACCENT).add_modifier(Modifier::SLOW_BLINK)),
        ])]
    };

    let _ = prompt_text; // suppress unused warning

    let block = Block::default()
        .title(" Import — paste ZIP or folder path (Enter = go, Esc = cancel) ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    f.render_widget(Paragraph::new(content).block(block), popup);
}

// ── Hint bar ──────────────────────────────────────────────────────────────────

fn draw_hint_bar(f: &mut Frame, area: Rect, screen: Screen, running: bool, filter_mode: bool) {
    let hints = if filter_mode {
        " type to filter   ↑↓ navigate   Enter apply   Esc discard"
    } else {
        match screen {
            Screen::Import => " Enter confirm   Esc cancel",
            Screen::Working if running =>
                " x cancel   s settings   PgUp/Dn scroll log   ? help   q quit",
            _ =>
                " Enter dl   a all   / filter   d delete   m m3u   s settings   ? help   q quit",
        }
    };

    f.render_widget(
        Paragraph::new(hints).style(Style::default().fg(DIM)),
        area,
    );
}

// ── Help popup ────────────────────────────────────────────────────────────────

/// Small centered yes/no popup for playlist deletion.
fn draw_confirm_popup(f: &mut Frame, area: Rect, name: &str) {
    let popup_w = (name.chars().count() as u16 + 24).clamp(44, area.width.saturating_sub(4));
    let popup_h = 5u16;
    let x = area.width.saturating_sub(popup_w) / 2;
    let y = area.height.saturating_sub(popup_h) / 2;
    let popup = Rect { x, y, width: popup_w, height: popup_h };

    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(vec![
            Span::raw("  Remove playlist "),
            Span::styled(format!("'{}'", name), Style::default().fg(WARN).add_modifier(Modifier::BOLD)),
            Span::raw("?"),
        ]),
        Line::from(Span::styled(
            "  Deletes the list, M3U and history — audio files stay.",
            Style::default().fg(DIM),
        )),
        Line::from(vec![
            Span::styled("  y", Style::default().fg(ERR).add_modifier(Modifier::BOLD)),
            Span::raw(" confirm   "),
            Span::styled("any other key", Style::default().fg(ACCENT)),
            Span::raw(" cancel"),
        ]),
    ];

    let block = Block::default()
        .title(" ── Delete? ── ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ERR));

    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn draw_help_popup(f: &mut Frame, area: Rect) {
    let popup_w = 50u16;
    let popup_h = 22u16;
    let x = area.width.saturating_sub(popup_w) / 2;
    let y = area.height.saturating_sub(popup_h) / 2;
    let popup = Rect {
        x:      x.max(1),
        y:      y.max(1),
        width:  popup_w.min(area.width.saturating_sub(2)),
        height: popup_h.min(area.height.saturating_sub(2)),
    };

    f.render_widget(Clear, popup);

    let key = |k: &'static str| Span::styled(
        format!("  {:<10}", k),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    );
    let desc = |d: &'static str| Span::raw(d);

    let lines = vec![
        Line::from(""),
        Line::from(vec![key("↑↓ / jk"),    desc("navigate playlists")]),
        Line::from(vec![key("Enter"),       desc("download selected")]),
        Line::from(vec![key("a"),           desc("download all playlists")]),
        Line::from(vec![key("x"),           desc("cancel running download")]),
        Line::from(vec![key("/"),           desc("search playlists (Esc clears)")]),
        Line::from(vec![key("d / Del"),     desc("remove selected playlist")]),
        Line::from(vec![key("i"),           desc("import Exportify ZIP")]),
        Line::from(vec![key("m"),           desc("generate M3U files")]),
        Line::from(vec![key("s"),           desc("settings")]),
        Line::from(vec![key("r"),           desc("rescan disk statuses")]),
        Line::from(vec![key("l"),           desc("save log to file")]),
        Line::from(vec![key("PgUp / PgDn"), desc("scroll log")]),
        Line::from(vec![key("q / Ctrl+C"),  desc("quit")]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  colors: ", Style::default().fg(DIM)),
            Span::styled("done ", Style::default().fg(OK)),
            Span::styled("partial ", Style::default().fg(WARN)),
            Span::styled("failed ", Style::default().fg(ERR)),
            Span::styled("new", Style::default().fg(Color::White)),
        ]),
        Line::from(
            Span::styled("  any key to dismiss", Style::default().fg(DIM))
        ),
    ];

    let block = Block::default()
        .title(" ── Keyboard Shortcuts ── ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    f.render_widget(Paragraph::new(lines).block(block), popup);
}

// ── Layout helpers ────────────────────────────────────────────────────────────

fn hsplit<const N: usize>(area: Rect, pcts: [u16; N]) -> [Rect; N] {
    let constraints: Vec<Constraint> = pcts.iter()
        .map(|p| Constraint::Percentage(*p))
        .collect();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    std::array::from_fn(|i| chunks[i])
}

fn vsplit<const N: usize>(area: Rect, constraints: [Constraint; N]) -> [Rect; N] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.to_vec())
        .split(area);
    std::array::from_fn(|i| chunks[i])
}
