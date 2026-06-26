pub mod render;
pub mod settings;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::config::{save, Config};
use crate::download::{run_all, Event as DownloadEvent};

// ── Log line ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LogLine {
    pub ts:   String,
    pub text: String,
}

impl LogLine {
    fn now(text: impl Into<String>) -> Self {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        LogLine { ts, text: text.into() }
    }
}

// ── Screen ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Home / playlist selector
    Home,
    /// Active download / M3U generation in progress
    Working,
    /// Import path input
    Import,
    /// Settings editor
    Settings,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub cfg:            Config,
    pub screen:         Screen,
    pub logs:           VecDeque<LogLine>,
    pub log_scroll:     usize,
    pub playlist_sel:   usize,
    pub playlist_names: Vec<String>,
    pub running:        bool,
    pub progress_label: String,
    pub progress_pct:   u16,
    /// Text being typed in the import input bar
    pub import_buf:     String,

    rt_handle: tokio::runtime::Handle,
    ev_rx:     Option<mpsc::UnboundedReceiver<DownloadEvent>>,
}

impl App {
    fn new(cfg: Config, rt: tokio::runtime::Handle) -> Self {
        let playlist_names = gather_playlist_names();
        App {
            cfg,
            screen:         Screen::Home,
            logs:           VecDeque::with_capacity(2000),
            log_scroll:     0,
            playlist_sel:   0,
            playlist_names,
            running:        false,
            progress_label: String::new(),
            progress_pct:   0,
            import_buf:     String::new(),
            rt_handle:      rt,
            ev_rx:          None,
        }
    }

    fn push_log(&mut self, text: impl Into<String>) {
        let line = LogLine::now(text);
        if self.logs.len() >= 2000 { self.logs.pop_front(); }
        self.logs.push_back(line);
        // Always scroll to bottom on new log line
        self.log_scroll = self.logs.len().saturating_sub(1);
    }

    fn refresh_playlists(&mut self) {
        self.playlist_names = gather_playlist_names();
        if self.playlist_sel >= self.playlist_names.len() {
            self.playlist_sel = 0;
        }
    }

    // ── Event draining ────────────────────────────────────────────────────────

    fn drain_events(&mut self) {
        loop {
            let ev = match self.ev_rx.as_mut() {
                Some(rx) => match rx.try_recv() { Ok(e) => e, Err(_) => break },
                None     => break,
            };
            self.handle_event(ev);
        }
    }

    fn handle_event(&mut self, ev: DownloadEvent) {
        match ev {
            DownloadEvent::Log(s) => self.push_log(s),

            DownloadEvent::PlaylistStart { name, index, total } => {
                self.push_log(format!("[{}/{}] ▶ {}", index, total, name));
                self.progress_label = name.clone();
                self.progress_pct   = ((index - 1) as u16 * 100) / (total as u16).max(1);
            }

            DownloadEvent::TrackDone { artist, title, status } => {
                self.push_log(format!("  {} — {} [{}]", artist, title, status));
            }

            DownloadEvent::PlaylistDone { name } => {
                self.push_log(format!("  ✔ {} done", name));
            }

            DownloadEvent::AllDone { stats } => {
                self.push_log(format!(
                    "━━ Done — {} found  {} not found  {} failed  {} quality warns",
                    stats.ok, stats.not_found, stats.failed, stats.quality_warns,
                ));
                self.running      = false;
                self.progress_pct = 100;
                self.screen       = Screen::Home;
                if self.cfg.notifications.enabled && self.cfg.notifications.on_completion {
                    crate::notify::send(
                        "spotify-to-offline",
                        &format!("Done — {}/{} tracks found", stats.ok, stats.total),
                    );
                }
            }

            DownloadEvent::ImportDone { count } => {
                if count == 0 {
                    self.push_log("  ⚠ No playlists found — press [i] and paste your Exportify ZIP path");
                } else {
                    self.push_log(format!("  ✓ {} playlist(s) imported and ready", count));
                }
                self.running = false;
                self.progress_label = String::new();
                self.refresh_playlists();
                self.screen = Screen::Home;
            }

            DownloadEvent::M3uDone => {
                self.push_log("  ✓ M3U playlists written");
                self.running      = false;
                self.progress_pct = 100;
                self.screen       = Screen::Home;
            }
        }
    }

    // ── Actions ───────────────────────────────────────────────────────────────

    fn start_download(&mut self, only: Option<String>) {
        if self.running { return; }
        let (tx, rx) = mpsc::unbounded_channel::<DownloadEvent>();
        self.ev_rx      = Some(rx);
        self.running    = true;
        self.progress_pct   = 0;
        self.progress_label = "Starting…".into();
        self.push_log("⬇ Download started");
        self.screen = Screen::Working;

        let cfg = self.cfg.clone();
        self.rt_handle.spawn(async move {
            let _ = run_all(&cfg, only.as_deref(), Some(tx)).await;
        });
    }

    fn start_import(&mut self, path: Option<String>) {
        if self.running { return; }
        let (tx, rx) = mpsc::unbounded_channel::<DownloadEvent>();
        self.ev_rx      = Some(rx);
        self.running    = true;
        self.progress_label = "Importing…".into();
        self.progress_pct   = 0;

        let msg = match &path {
            Some(p) => format!("⬇ Importing from {}…", p),
            None    => "⬇ Auto-detecting playlists…".into(),
        };
        self.push_log(msg);
        self.screen = Screen::Working;

        self.rt_handle.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let log_tx = tx.clone();
                let log    = move |s: String| { let _ = log_tx.send(DownloadEvent::Log(s)); };

                let count = match path {
                    Some(p) => crate::import::import_path(
                        &std::path::PathBuf::from(&p), log,
                    ),
                    None => crate::import::auto_detect(log),
                };
                (count, tx)
            }).await;

            if let Ok((count_result, tx)) = result {
                let count = count_result.unwrap_or(0);
                let _ = tx.send(DownloadEvent::ImportDone { count });
            }
        });
    }

    fn start_m3u(&mut self) {
        if self.running { return; }
        let (tx, rx) = mpsc::unbounded_channel::<DownloadEvent>();
        self.ev_rx      = Some(rx);
        self.running    = true;
        self.progress_label = "M3U".into();
        self.progress_pct   = 0;
        self.push_log("♪ Generating M3U playlists…");
        self.screen = Screen::Working;

        let cfg = self.cfg.clone();
        self.rt_handle.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let log_tx = tx.clone();
                let log    = move |s: String| { let _ = log_tx.send(DownloadEvent::Log(s)); };
                let r = crate::m3u::run_with_log(&cfg, None, log);
                (r, tx)
            }).await;

            if let Ok((r, tx)) = result {
                match r {
                    Ok(())  => { let _ = tx.send(DownloadEvent::M3uDone); }
                    Err(e)  => {
                        let _ = tx.send(DownloadEvent::Log(format!("  ✗ M3U error: {}", e)));
                        let _ = tx.send(DownloadEvent::M3uDone);
                    }
                }
            }
        });
    }
}

fn gather_playlist_names() -> Vec<String> {
    match crate::import::list_playlists() {
        Ok(v) => v.into_iter()
            .map(|p| p.file_stem().unwrap_or_default().to_string_lossy().into_owned())
            .collect(),
        Err(_) => vec![],
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run_tui(cfg: Config) -> Result<()> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || run_tui_blocking(cfg, handle))
        .await
        .map_err(|e| anyhow::anyhow!("TUI thread panicked: {:?}", e))?
}

fn run_tui_blocking(cfg: Config, rt: tokio::runtime::Handle) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend  = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let mut app            = App::new(cfg, rt);
    let mut settings_state = settings::SettingsState::new(&app.cfg);
    let tick               = Duration::from_millis(50);

    // Auto-detect playlists on startup if none found yet
    if app.playlist_names.is_empty() {
        app.start_import(None);
    }

    loop {
        app.drain_events();

        term.draw(|f| match app.screen {
            Screen::Home | Screen::Working => render::draw_main(f, &app),
            Screen::Import                 => render::draw_main(f, &app), // import bar overlay
            Screen::Settings               => settings::draw_settings(f, &app, &mut settings_state),
        })?;

        let now = Instant::now();
        if event::poll(tick.saturating_sub(now.elapsed()))? {
            if let Event::Key(key) = event::read()? {
                // Global quit (not while typing in import or settings)
                if key.code == KeyCode::Char('q')
                    && key.modifiers.is_empty()
                    && app.screen != Screen::Settings
                    && app.screen != Screen::Import
                {
                    break;
                }
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }

                match app.screen {
                    Screen::Home | Screen::Working => handle_main_key(&mut app, key),
                    Screen::Import                 => handle_import_key(&mut app, key),
                    Screen::Settings => {
                        if settings::handle_key(&mut app, &mut settings_state, key) {
                            if let Err(e) = save(&app.cfg) {
                                app.push_log(format!("⚠ Save failed: {}", e));
                            }
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

// ── Main screen input ─────────────────────────────────────────────────────────

fn handle_main_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;
    match key.code {
        Up | Char('k') => {
            if app.playlist_sel > 0 { app.playlist_sel -= 1; }
        }
        Down | Char('j') => {
            if app.playlist_sel + 1 < app.playlist_names.len() {
                app.playlist_sel += 1;
            }
        }
        Enter => {
            if app.running { return; }
            if app.playlist_names.is_empty() {
                // Enter with no playlists = open import
                app.screen = Screen::Import;
                return;
            }
            let only = app.playlist_names.get(app.playlist_sel).cloned();
            app.start_download(only);
        }
        Char('a') => {
            if !app.running { app.start_download(None); }
        }
        Char('i') => {
            if !app.running {
                app.import_buf.clear();
                app.screen = Screen::Import;
            }
        }
        Char('m') => {
            if !app.running { app.start_m3u(); }
        }
        Char('s') => {
            app.screen = Screen::Settings;
        }
        PageUp => {
            app.log_scroll = app.log_scroll.saturating_sub(10);
        }
        PageDown => {
            let max = app.logs.len().saturating_sub(1);
            app.log_scroll = (app.log_scroll + 10).min(max);
        }
        Esc => {
            if app.screen == Screen::Working && !app.running {
                app.screen = Screen::Home;
            }
        }
        _ => {}
    }
}

// ── Import input ──────────────────────────────────────────────────────────────

fn handle_import_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.import_buf.clear();
            app.screen = Screen::Home;
        }
        KeyCode::Enter => {
            let path = app.import_buf.trim().to_string();
            app.import_buf.clear();
            if path.is_empty() {
                // blank = auto-detect
                app.start_import(None);
            } else {
                app.start_import(Some(path));
            }
        }
        KeyCode::Backspace => {
            app.import_buf.pop();
        }
        KeyCode::Char(c) => {
            app.import_buf.push(c);
        }
        _ => {}
    }
}

// ── Public façade for main.rs ─────────────────────────────────────────────────

pub async fn run() -> Result<()> {
    let cfg = crate::config::load()?;
    run_tui(cfg).await
}
