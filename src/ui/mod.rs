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
    fn new(text: impl Into<String>) -> Self {
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        LogLine { ts: now, text: text.into() }
    }
}

// ── Screen ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Working,
    Settings,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub cfg:           Config,
    pub screen:        Screen,
    pub logs:          VecDeque<LogLine>,
    pub log_scroll:    usize,
    pub playlist_sel:  usize,
    pub playlist_names: Vec<String>,
    pub running:       bool,
    pub progress_label: String,
    pub progress_pct:  u16,

    /// Runtime handle so blocking TUI thread can spawn async tasks
    rt_handle:  tokio::runtime::Handle,
    /// Receives events from background download task
    ev_rx:      Option<tokio::sync::mpsc::UnboundedReceiver<DownloadEvent>>,
}

impl App {
    fn new(cfg: Config, rt_handle: tokio::runtime::Handle) -> Self {
        let playlist_names = gather_playlist_names();
        App {
            cfg,
            screen: Screen::Menu,
            logs:   VecDeque::with_capacity(1000),
            log_scroll:    0,
            playlist_sel:  0,
            playlist_names,
            running:       false,
            progress_label: String::new(),
            progress_pct:  0,
            rt_handle,
            ev_rx: None,
        }
    }

    fn push_log(&mut self, text: impl Into<String>) {
        if self.logs.len() >= 1000 { self.logs.pop_front(); }
        self.logs.push_back(LogLine::new(text));
        self.log_scroll = self.logs.len().saturating_sub(1);
    }

    fn drain_events(&mut self) {
        loop {
            let ev = match self.ev_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(e)  => e,
                    Err(_) => break,
                },
                None => break,
            };
            self.handle_download_event(ev);
        }
    }

    fn handle_download_event(&mut self, ev: DownloadEvent) {
        match ev {
            DownloadEvent::Log(s) => self.push_log(s),

            DownloadEvent::PlaylistStart { name, index, total } => {
                self.push_log(format!("[{}/{}] ▶ {}", index, total, name));
                self.progress_label = name;
            }

            DownloadEvent::TrackDone { artist, title, status } => {
                self.push_log(format!("  {} — {} [{}]", artist, title, status));
            }

            DownloadEvent::PlaylistDone { name } => {
                self.push_log(format!("  ✔ {} done", name));
            }

            DownloadEvent::AllDone { stats } => {
                self.push_log(format!(
                    "━━ Complete: {} ok  {} not found  {} failed  {} quality warns",
                    stats.ok, stats.not_found, stats.failed, stats.quality_warns,
                ));
                self.running      = false;
                self.progress_pct = 100;
                if self.cfg.notifications.enabled && self.cfg.notifications.on_completion {
                    crate::notify::send(
                        "spotify-to-offline",
                        &format!("Done — {}/{} tracks found", stats.ok, stats.total),
                    );
                }
            }
        }
    }

    fn start_download(&mut self, only: Option<String>) {
        if self.running { return; }

        let (tx, rx) = mpsc::unbounded_channel::<DownloadEvent>();
        self.ev_rx      = Some(rx);
        self.running    = true;
        self.progress_pct   = 0;
        self.progress_label = "Starting…".into();
        self.push_log("⬇ Download started");

        let cfg2 = self.cfg.clone();
        self.rt_handle.spawn(async move {
            let only_str = only;
            let _ = run_all(&cfg2, only_str.as_deref(), Some(tx)).await;
        });
    }
}

fn gather_playlist_names() -> Vec<String> {
    match crate::import::list_playlists() {
        Ok(v) => v.into_iter()
            .map(|p| p.file_stem().unwrap_or_default().to_string_lossy().to_string())
            .collect(),
        Err(_) => vec![],
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run the TUI. Must be called from within a tokio runtime context (e.g. #[tokio::main]).
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

    loop {
        app.drain_events();

        term.draw(|f| match app.screen {
            Screen::Menu | Screen::Working => render::draw_main(f, &app),
            Screen::Settings              => settings::draw_settings(f, &app, &mut settings_state),
        })?;

        let now = Instant::now();
        if event::poll(tick.saturating_sub(now.elapsed()))? {
            if let Event::Key(key) = event::read()? {
                // Global quit
                if key.code == KeyCode::Char('q') && key.modifiers.is_empty()
                    && app.screen != Screen::Settings
                {
                    break;
                }
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }

                match app.screen {
                    Screen::Menu | Screen::Working => handle_main_key(&mut app, key),
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
            if app.screen == Screen::Working { return; }
            let only = app.playlist_names.get(app.playlist_sel).cloned();
            app.screen = Screen::Working;
            app.start_download(only);
        }
        Char('a') => {
            app.screen = Screen::Working;
            app.start_download(None);
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
                app.screen = Screen::Menu;
            }
        }
        _ => {}
    }
}


// ── Public façade for main.rs ─────────────────────────────────────────────────

pub async fn run() -> Result<()> {
    let cfg = crate::config::load()?;
    run_tui(cfg).await
}
