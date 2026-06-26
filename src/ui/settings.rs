use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::App;
use crate::config::Config;

// ── Colours ───────────────────────────────────────────────────────────────────

const ACCENT: Color = Color::Cyan;
const DIM:    Color = Color::DarkGray;
const EDIT:   Color = Color::Yellow;

// ── Setting entry ─────────────────────────────────────────────────────────────

struct Entry {
    label:   &'static str,
    value:   String,
    editable: bool,
}

impl Entry {
    fn new(label: &'static str, value: impl ToString) -> Self {
        Entry { label, value: value.to_string(), editable: true }
    }
    fn ro(label: &'static str, value: impl ToString) -> Self {
        Entry { label, value: value.to_string(), editable: false }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct SettingsState {
    pub selected:  usize,
    pub edit_mode: bool,
    pub input_buf: String,
    entries:       Vec<Entry>,
}

impl SettingsState {
    pub fn new(cfg: &Config) -> Self {
        SettingsState {
            selected:  0,
            edit_mode: false,
            input_buf: String::new(),
            entries:   build_entries(cfg),
        }
    }

    fn sync_from_cfg(&mut self, cfg: &Config) {
        self.entries = build_entries(cfg);
    }
}

fn build_entries(cfg: &Config) -> Vec<Entry> {
    vec![
        Entry::new("Music root",          cfg.paths.music_root.display()),
        Entry::new("Playlists dir",        cfg.paths.playlists_dir.display()),
        Entry::new("yt-dlp path",          &cfg.paths.ytdlp_path),
        Entry::new("Soulseek username",    &cfg.soulseek.username),
        Entry::new("Provider order",       cfg.provider.order.join(", ")),
        Entry::new("Fallback enabled",     cfg.provider.fallback_enabled),
        Entry::new("Preferred format",     &cfg.download.preferred_format),
        Entry::new("Concurrent playlists", cfg.download.concurrent_playlists),
        Entry::new("Concurrent tracks",    cfg.download.concurrent_tracks),
        Entry::new("Notifications",        cfg.notifications.enabled),
        Entry::new("DAP profile",          cfg.dap_profiles.first().map(|p| p.name.as_str()).unwrap_or("none")),
        Entry::ro("Quality warnings",      cfg.download.quality_warning),
    ]
}

// ── Key handling ──────────────────────────────────────────────────────────────

/// Returns true when settings screen should exit (Esc at top level).
pub fn handle_key(app: &mut App, state: &mut SettingsState, key: KeyEvent) -> bool {
    if state.edit_mode {
        match key.code {
            KeyCode::Enter => {
                apply_edit(app, state);
                state.edit_mode = false;
                state.sync_from_cfg(&app.cfg);
                return true; // save after every edit
            }
            KeyCode::Esc => {
                state.edit_mode = false;
                state.input_buf.clear();
            }
            KeyCode::Char(c) => state.input_buf.push(c),
            KeyCode::Backspace => { state.input_buf.pop(); }
            _ => {}
        }
        return false;
    }

    // Normal navigation
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.selected > 0 { state.selected -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.selected + 1 < state.entries.len() { state.selected += 1; }
        }
        KeyCode::Enter => {
            if state.entries[state.selected].editable {
                state.input_buf = state.entries[state.selected].value.clone();
                state.edit_mode = true;
            }
        }
        KeyCode::Esc | KeyCode::Char('s') => {
            app.screen = super::Screen::Menu;
            return true;
        }
        _ => {}
    }
    false
}

fn apply_edit(app: &mut App, state: &SettingsState) {
    let val = state.input_buf.trim().to_string();
    match state.selected {
        0  => app.cfg.paths.music_root        = val.into(),
        1  => app.cfg.paths.playlists_dir     = val.into(),
        2  => app.cfg.paths.ytdlp_path        = val,
        3  => app.cfg.soulseek.username        = val,
        4  => {
            app.cfg.provider.order = val.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        5  => app.cfg.provider.fallback_enabled   = parse_bool(&val),
        6  => app.cfg.download.preferred_format   = val,
        7  => app.cfg.download.concurrent_playlists = val.parse().unwrap_or(2),
        8  => app.cfg.download.concurrent_tracks  = val.parse().unwrap_or(4),
        9  => app.cfg.notifications.enabled       = parse_bool(&val),
        10 => {
            // Move chosen profile to front
            app.cfg.dap_profiles.sort_by_key(|p| if p.name == val { 0i32 } else { 1 });
        }
        _  => {}
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "true" | "yes" | "1" | "y")
}

// ── Render ────────────────────────────────────────────────────────────────────

pub fn draw_settings(f: &mut Frame, _app: &App, state: &mut SettingsState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    // List of settings
    let block = Block::default()
        .title("Settings  (↑↓ navigate · Enter edit · Esc/s save & close)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));

    let items: Vec<ListItem> = state.entries.iter().enumerate().map(|(i, e)| {
        let selected = i == state.selected;
        let val_text = if selected && state.edit_mode {
            format!("{}_", state.input_buf)
        } else {
            e.value.clone()
        };

        let label_style = Style::default()
            .fg(if selected { ACCENT } else { Color::White })
            .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() });

        let val_style = Style::default().fg(if state.edit_mode && selected { EDIT } else { DIM });

        ListItem::new(Line::from(vec![
            Span::styled(format!("{:<26}", e.label), label_style),
            Span::styled(val_text, val_style),
        ]))
    }).collect();

    let list = List::new(items).block(block);
    f.render_widget(list, chunks[0]);

    // Hint bar
    let hint_text = if state.edit_mode {
        "  Editing — Enter to confirm · Esc to cancel"
    } else {
        "  Enter to edit · Esc or [s] to save & return to menu"
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(DIM))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(DIM)));
    f.render_widget(hint, chunks[1]);
}
