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
    /// Stable identifier used by apply_edit — order in the list doesn't matter.
    key:       &'static str,
    label:     &'static str,
    value:     String,
    editable:  bool,
    /// Action entries run a side-effect on Enter instead of opening an editor.
    is_action: bool,
    /// Password fields — display as bullets, mask the edit buffer too.
    masked:    bool,
}

impl Entry {
    fn new(key: &'static str, label: &'static str, value: impl ToString) -> Self {
        Entry { key, label, value: value.to_string(), editable: true,  is_action: false, masked: false }
    }
    fn action(key: &'static str, label: &'static str, value: impl ToString) -> Self {
        Entry { key, label, value: value.to_string(), editable: false, is_action: true,  masked: false }
    }
    fn secret(key: &'static str, label: &'static str, value: impl ToString) -> Self {
        Entry { key, label, value: value.to_string(), editable: true,  is_action: false, masked: true  }
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
        Entry::new("music_root",     "Music root",           cfg.paths.music_root.display()),
        Entry::new("playlists_dir",  "Playlists dir",        cfg.paths.playlists_dir.display()),
        Entry::new("ytdlp_path",     "yt-dlp path",          &cfg.paths.ytdlp_path),
        Entry::new("slsk_user",      "Soulseek username",    &cfg.soulseek.username),
        Entry::secret("slsk_pass",   "Soulseek password",    &cfg.soulseek.password),
        Entry::new("provider_order", "Provider order",       cfg.provider.order.join(", ")),
        Entry::new("fallback",       "Fallback enabled",     cfg.provider.fallback_enabled),
        Entry::new("pref_format",    "Preferred format",     &cfg.download.preferred_format),
        Entry::new("name_format",    "File name format",     &cfg.download.name_format),
        Entry::new("conc_playlists", "Concurrent playlists", cfg.download.concurrent_playlists),
        Entry::new("conc_tracks",    "Concurrent tracks",    cfg.download.concurrent_tracks),
        Entry::new("notifications",  "Notifications",        cfg.notifications.enabled),
        Entry::new("dap_profile",    "DAP profile",          cfg.dap_profiles.first().map(|p| p.name.as_str()).unwrap_or("none")),
        Entry::new("quality_warn",   "Quality warnings",     cfg.download.quality_warning),
        Entry::new("verbose_logs",   "Verbose logs",         cfg.verbose_logs),
        Entry::new("auto_save_log",  "Auto-save log",        cfg.auto_save_log),
        // ── Actions ──────────────────────────────────────────────────────────
        Entry::action(
            "dl_sldl",
            "Download sldl",
            if crate::sldl_setup::sldl_found() {
                "✓ installed — Enter to re-download"
            } else {
                "not found — press Enter to auto-download"
            },
        ),
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
            let entry = &state.entries[state.selected];
            if entry.is_action {
                // Dispatch action entries
                if entry.key == "dl_sldl" {
                    app.start_sldl_download();
                }
            } else if entry.editable {
                state.input_buf = entry.value.clone();
                state.edit_mode = true;
            }
        }
        KeyCode::Esc | KeyCode::Char('s') => {
            app.screen = app.settings_return;
            if app.running {
                // Settings were edited mid-run; the active download keeps its
                // old config — changes apply from the next run.
            }
            return true;
        }
        _ => {}
    }
    false
}

fn apply_edit(app: &mut App, state: &SettingsState) {
    let val = state.input_buf.trim().to_string();
    match state.entries[state.selected].key {
        "music_root"     => app.cfg.paths.music_root    = val.into(),
        "playlists_dir"  => app.cfg.paths.playlists_dir = val.into(),
        "ytdlp_path"     => app.cfg.paths.ytdlp_path    = val,
        "slsk_user"      => app.cfg.soulseek.username   = val,
        "slsk_pass"      => app.cfg.soulseek.password   = val,
        "provider_order" => {
            app.cfg.provider.order = val.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        "fallback"       => app.cfg.provider.fallback_enabled     = parse_bool(&val),
        "pref_format"    => app.cfg.download.preferred_format     = val,
        "name_format"    => {
            app.cfg.download.name_format = if val.is_empty() {
                crate::config::default_name_format()
            } else {
                val
            };
        }
        "conc_playlists" => app.cfg.download.concurrent_playlists = val.parse().unwrap_or(2),
        "conc_tracks"    => app.cfg.download.concurrent_tracks    = val.parse().unwrap_or(4),
        "notifications"  => app.cfg.notifications.enabled         = parse_bool(&val),
        "dap_profile"    => {
            // Move chosen profile to front
            app.cfg.dap_profiles.sort_by_key(|p| if p.name == val { 0i32 } else { 1 });
        }
        "quality_warn"   => app.cfg.download.quality_warning = parse_bool(&val),
        "verbose_logs"   => app.cfg.verbose_logs             = parse_bool(&val),
        "auto_save_log"  => app.cfg.auto_save_log            = parse_bool(&val),
        _ => {}
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "true" | "yes" | "1" | "y")
}

// ── Render ────────────────────────────────────────────────────────────────────

pub fn draw_settings(f: &mut Frame, app: &App, state: &mut SettingsState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    // List of settings
    let title = if app.running {
        "Settings — download running; changes apply to the NEXT run"
    } else {
        "Settings  (↑↓ navigate · Enter edit · Esc/s save & close)"
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));

    let items: Vec<ListItem> = state.entries.iter().enumerate().map(|(i, e)| {
        let selected = i == state.selected;
        let val_text = if selected && state.edit_mode {
            // Password fields: show bullets while typing
            if e.masked {
                format!("{}_", "•".repeat(state.input_buf.len()))
            } else {
                format!("{}_", state.input_buf)
            }
        } else if e.masked && !e.value.is_empty() {
            "•".repeat(e.value.len().min(24))
        } else {
            e.value.clone()
        };

        let label_style = Style::default()
            .fg(if selected { ACCENT } else { Color::White })
            .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() });

        let val_style = if e.is_action {
            Style::default().fg(if selected { ACCENT } else { DIM })
        } else {
            Style::default().fg(if state.edit_mode && selected { EDIT } else { DIM })
        };

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
        "  Enter to edit · Esc or [s] to save & return"
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(DIM))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(DIM)));
    f.render_widget(hint, chunks[1]);
}
