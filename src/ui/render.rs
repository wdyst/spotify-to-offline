use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph},
};

use super::{App, Screen};

// ── Palette ───────────────────────────────────────────────────────────────────

const ACCENT:   Color = Color::Cyan;
const DIM:      Color = Color::DarkGray;
const OK:       Color = Color::Green;
const WARN:     Color = Color::Yellow;
const ERR:      Color = Color::Red;
const PROGRESS: Color = Color::Cyan;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw_main(f: &mut Frame, app: &App) {
    let area = f.area();

    // Reserve one line at the bottom for the hint bar
    let [main, hint_row] = vsplit(area, [Constraint::Min(0), Constraint::Length(1)]);

    // Horizontal split: playlist list on the left, log + gauge on the right
    let [left, right] = hsplit(main, [28, 72]);

    // Right side: scrollable log on top, progress gauge on the bottom
    let [log_area, gauge_area] = vsplit(right, [Constraint::Min(3), Constraint::Length(3)]);

    draw_playlist_panel(f, app, left);
    draw_log_panel(f, app, log_area);
    draw_gauge(f, app, gauge_area);
    draw_hint_bar(f, app, hint_row);
}

// ── Playlist panel ────────────────────────────────────────────────────────────

fn draw_playlist_panel(f: &mut Frame, app: &App, area: Rect) {
    let title = if app.screen == Screen::Working && app.running {
        " Downloading… "
    } else {
        " Playlists "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    if app.playlist_names.is_empty() {
        let msg = Paragraph::new("No playlists yet.\nRun `s2o import` first.")
            .style(Style::default().fg(DIM))
            .block(block);
        f.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = app.playlist_names.iter().enumerate().map(|(i, name)| {
        let selected = i == app.playlist_sel;
        let style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(name.as_str()).style(style)
    }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_symbol("▶ ");
    f.render_widget(list, area);
}

// ── Log panel ─────────────────────────────────────────────────────────────────

fn draw_log_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Log ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    let inner_h = area.height.saturating_sub(2) as usize;

    // Calculate which log lines to show (scroll to bottom by default)
    let total = app.logs.len();
    let start = if total > inner_h {
        app.log_scroll.min(total - inner_h)
    } else {
        0
    };

    let lines: Vec<Line> = app.logs
        .iter()
        .skip(start)
        .take(inner_h)
        .map(|entry| {
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", entry.ts),
                    Style::default().fg(DIM),
                ),
                Span::styled(
                    entry.text.clone(),
                    classify_log_style(&entry.text),
                ),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

/// Map a log line's content to a display colour.
fn classify_log_style(text: &str) -> Style {
    // Check for prefix icons and key phrases (ordered most-specific first)
    if text.contains("━━") {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else if text.contains('✓') || text.contains("ok [") {
        Style::default().fg(OK)
    } else if text.contains('⚠') || text.contains("quality_warn") || text.contains("warn") {
        Style::default().fg(WARN)
    } else if text.contains('✗') || text.contains("[err]") || text.contains("failed") || text.contains("error") {
        Style::default().fg(ERR)
    } else if text.contains('▶') || text.starts_with('[') {
        Style::default().fg(ACCENT)
    } else if text.contains("not found") {
        Style::default().fg(DIM)
    } else {
        Style::default().fg(Color::White)
    }
}

// ── Progress gauge ────────────────────────────────────────────────────────────

fn draw_gauge(f: &mut Frame, app: &App, area: Rect) {
    let pct   = app.progress_pct;
    let label = if app.progress_label.is_empty() {
        "Idle".to_string()
    } else {
        format!("{}  {}%", app.progress_label, pct)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    let gauge = Gauge::default()
        .block(block)
        .gauge_style(Style::default().fg(PROGRESS))
        .percent(pct)
        .label(label);

    f.render_widget(gauge, area);
}

// ── Hint bar ──────────────────────────────────────────────────────────────────

fn draw_hint_bar(f: &mut Frame, app: &App, area: Rect) {
    let hints = if app.screen == Screen::Working && app.running {
        " ↑↓ scroll log   PgUp/Dn page   q quit"
    } else {
        " ↑↓ navigate   Enter download   a all   s settings   q quit"
    };

    let bar = Paragraph::new(hints)
        .style(Style::default().fg(DIM));
    f.render_widget(bar, area);
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
