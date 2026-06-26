use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph},
};

use super::{App, Screen};

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
    draw_hint_bar(f, hint_row, app.screen, app.running);

    // Import input overlay — drawn on top of everything
    if app.screen == Screen::Import {
        draw_import_bar(f, area, app);
    }
}

// ── Playlist panel ────────────────────────────────────────────────────────────

fn draw_playlist_panel(f: &mut Frame, app: &App, area: Rect) {
    if app.playlist_names.is_empty() {
        draw_home_menu(f, app, area);
        return;
    }

    let title = if app.running {
        match app.screen {
            Screen::Working => " Downloading… ",
            _               => " Playlists ",
        }
    } else {
        " Playlists "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    let items: Vec<ListItem> = app.playlist_names.iter().enumerate().map(|(i, name)| {
        let style = if i == app.playlist_sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(name.as_str()).style(style)
    }).collect();

    let list = List::new(items).block(block).highlight_symbol("▶ ");
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

fn draw_log_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Log ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    let inner_h = area.height.saturating_sub(2) as usize;
    let total   = app.logs.len();
    let start   = if total > inner_h {
        app.log_scroll.min(total - inner_h)
    } else {
        0
    };

    let lines: Vec<Line> = app.logs
        .iter()
        .skip(start)
        .take(inner_h)
        .map(|entry| Line::from(vec![
            Span::styled(format!("[{}] ", entry.ts), Style::default().fg(DIM)),
            Span::styled(entry.text.clone(), log_style(&entry.text)),
        ]))
        .collect();

    f.render_widget(Paragraph::new(lines).block(block), area);
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

fn draw_hint_bar(f: &mut Frame, area: Rect, screen: Screen, running: bool) {
    let hints = match screen {
        Screen::Import => " Enter confirm   Esc cancel",
        Screen::Working if running =>
            " ↑↓ scroll log   PgUp/Dn page   q quit",
        _ =>
            " ↑↓ navigate   Enter download   a all   i import   m m3u   s settings   q quit",
    };

    f.render_widget(
        Paragraph::new(hints).style(Style::default().fg(DIM)),
        area,
    );
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
