pub mod render;
pub mod settings;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::{HashMap, VecDeque};
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

// ── Playlist status ───────────────────────────────────────────────────────────

/// Scanned from music_root on startup and after each download.
#[derive(Clone, Copy, PartialEq)]
pub enum PlaylistStatus {
    /// Directory not found on disk.
    Unknown,
    /// Directory exists but contains no recognised audio files.
    Empty,
    /// Directory contains at least one audio file.
    HasFiles,
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
    /// Set to true after a successful sldl download — causes TUI to restart.
    pub should_restart: bool,

    // ── Animation ─────────────────────────────────────────────────────────────
    pub anim_frame:        u8,
    pub anim_last_tick:    Instant,

    // ── Playlist filter ───────────────────────────────────────────────────────
    pub playlist_filter:   String,
    pub filter_mode:       bool,

    // ── Help overlay ──────────────────────────────────────────────────────────
    pub show_help:         bool,

    // ── Per-playlist status (scanned from music_root) ─────────────────────────
    pub playlist_statuses: HashMap<String, PlaylistStatus>,

    // ── Name of playlist being downloaded right now ───────────────────────────
    pub downloading_name:  Option<String>,

    rt_handle: tokio::runtime::Handle,
    ev_rx:     Option<mpsc::UnboundedReceiver<DownloadEvent>>,
}

impl App {
    fn new(cfg: Config, rt: tokio::runtime::Handle) -> Self {
        let playlist_names    = gather_playlist_names();
        let playlist_statuses = scan_statuses(&cfg.paths.music_root, &playlist_names);
        App {
            cfg,
            screen:           Screen::Home,
            logs:             VecDeque::with_capacity(2000),
            log_scroll:       0,
            playlist_sel:     0,
            playlist_names,
            running:          false,
            progress_label:   String::new(),
            progress_pct:     0,
            import_buf:       String::new(),
            should_restart:   false,
            anim_frame:       0,
            anim_last_tick:   Instant::now(),
            playlist_filter:  String::new(),
            filter_mode:      false,
            show_help:        false,
            playlist_statuses,
            downloading_name: None,
            rt_handle:        rt,
            ev_rx:            None,
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
        self.rescan_statuses();
    }

    fn rescan_statuses(&mut self) {
        self.playlist_statuses = scan_statuses(
            &self.cfg.paths.music_root, &self.playlist_names,
        );
    }

    /// Returns only the playlist names that match the current filter (or all if no filter).
    pub fn visible_playlist_names(&self) -> Vec<&str> {
        if self.playlist_filter.is_empty() {
            self.playlist_names.iter().map(|s| s.as_str()).collect()
        } else {
            let f = self.playlist_filter.to_lowercase();
            self.playlist_names.iter()
                .filter(move |n| n.to_lowercase().contains(&f))
                .map(|s| s.as_str())
                .collect()
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
            DownloadEvent::Log(s) => {
                // In non-verbose mode, silently drop .NET exception spam from sldl.
                if self.cfg.verbose_logs || !is_log_noise(&s) {
                    self.push_log(s);
                }
            }

            DownloadEvent::PlaylistStart { name, index, total } => {
                self.push_log(format!("[{}/{}] ▶ {}", index, total, name));
                self.progress_label   = name.clone();
                self.downloading_name = Some(name);
                self.progress_pct     = ((index - 1) as u16 * 100) / (total as u16).max(1);
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
                self.progress_label   = format!(
                    "Done  ✓ {}  ✗ {}  ⚡ {}",
                    stats.ok, stats.not_found, stats.failed,
                );
                self.downloading_name = None;
                self.running          = false;
                self.progress_pct     = 100;
                self.screen           = Screen::Home;
                self.rescan_statuses();
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

            DownloadEvent::SldlDone { ok } => {
                if ok {
                    self.push_log("  ✓ sldl installed! Restarting s2o in 2 seconds…");
                    self.should_restart = true;
                } else {
                    self.push_log("  ✗ sldl download failed — grab it manually: github.com/fiso64/slsk-batchdl");
                }
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

    pub fn start_sldl_download(&mut self) {
        if self.running { return; }
        let (tx, rx) = mpsc::unbounded_channel::<DownloadEvent>();
        self.ev_rx          = Some(rx);
        self.running        = true;
        self.progress_label = "Downloading sldl…".into();
        self.progress_pct   = 0;
        self.push_log("⬇ Fetching sldl from github.com/fiso64/slsk-batchdl…");
        self.screen = Screen::Working;

        self.rt_handle.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let log_tx = tx.clone();
                let log    = move |s: String| { let _ = log_tx.send(DownloadEvent::Log(s)); };
                let r = crate::sldl_setup::download_sldl(log);
                (r, tx)
            }).await;

            if let Ok((r, tx)) = result {
                match r {
                    Ok(path) => {
                        let _ = tx.send(DownloadEvent::Log(
                            format!("  ✓ installed → {}", path.display()),
                        ));
                        let _ = tx.send(DownloadEvent::SldlDone { ok: true });
                    }
                    Err(e) => {
                        let _ = tx.send(DownloadEvent::Log(format!("  ✗ {}", e)));
                        let _ = tx.send(DownloadEvent::SldlDone { ok: false });
                    }
                }
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
    execute!(stdout, EnterAlternateScreen, event::EnableBracketedPaste)?;
    let backend  = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let mut app            = App::new(cfg, rt);
    let mut settings_state = settings::SettingsState::new(&app.cfg);
    let tick               = Duration::from_millis(50);

    // Startup sldl check
    if crate::sldl_setup::sldl_found() {
        app.push_log("✓ sldl found");
    } else {
        app.push_log("⚠ sldl not installed — press [s] › 'Download sldl' › Enter to auto-install");
    }

    // Auto-detect playlists on startup if none found yet
    if app.playlist_names.is_empty() {
        app.start_import(None);
    }

    loop {
        app.drain_events();

        // After a successful sldl install, show the message then restart
        if app.should_restart {
            term.draw(|f| render::draw_main(f, &app))?;
            std::thread::sleep(Duration::from_millis(2000));
            break;
        }

        // Advance the log-panel animation frame every 200 ms while downloading
        if app.running {
            let now = Instant::now();
            if now.duration_since(app.anim_last_tick) >= Duration::from_millis(200) {
                app.anim_frame      = app.anim_frame.wrapping_add(1);
                app.anim_last_tick  = now;
            }
        }

        term.draw(|f| match app.screen {
            Screen::Home | Screen::Working => render::draw_main(f, &app),
            Screen::Import                 => render::draw_main(f, &app),
            Screen::Settings               => settings::draw_settings(f, &app, &mut settings_state),
        })?;

        let now = Instant::now();
        if event::poll(tick.saturating_sub(now.elapsed()))? {
            match event::read()? {
                // Bracketed paste — append directly to import buffer
                Event::Paste(text) => {
                    if app.screen == Screen::Import {
                        app.import_buf.push_str(&text);
                    }
                }
                // Only handle key-press events; ignore release/repeat to avoid doubled input
                Event::Key(key) if key.kind == KeyEventKind::Press => {
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

                    // Help overlay: any key dismisses it ('?' also re-toggles via
                    // handle_main_key below, so here we just close it on other keys)
                    if app.show_help {
                        app.show_help = false;
                        if key.code != KeyCode::Char('?') {
                            continue; // key consumed to dismiss help
                        }
                        // '?' while help is open = close it (don't fall through to re-open)
                        continue;
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
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, event::DisableBracketedPaste)?;

    // Auto-save log if configured
    if app.cfg.auto_save_log {
        match write_log(&app.cfg.paths.music_root, &app.logs) {
            Ok(path) => println!("Log saved: {}", path.display()),
            Err(e)   => eprintln!("Log save failed: {}", e),
        }
    }

    // Re-exec after sldl install so the new binary is picked up immediately
    if app.should_restart {
        let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("s2o"));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let _ = std::process::Command::new(&exe)
                .args(std::env::args_os().skip(1))
                .exec(); // replaces current process image
        }
        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new(&exe)
                .args(std::env::args_os().skip(1))
                .spawn();
        }
    }

    Ok(())
}

// ── Main screen input ─────────────────────────────────────────────────────────

fn handle_main_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use KeyCode::*;

    // ── Filter mode: char keys feed the filter, arrows navigate, Esc exits ──
    if app.filter_mode {
        match key.code {
            Esc => {
                app.filter_mode = false;
                app.playlist_filter.clear();
                app.playlist_sel = 0;
            }
            Backspace => {
                app.playlist_filter.pop();
                app.playlist_sel = 0;
            }
            // Arrow keys navigate the filtered list (j/k type into filter)
            Up => {
                if app.playlist_sel > 0 { app.playlist_sel -= 1; }
            }
            Down => {
                let len = app.visible_playlist_names().len();
                if app.playlist_sel + 1 < len { app.playlist_sel += 1; }
            }
            Enter => {
                if app.running { return; }
                let only: Option<String> = {
                    let vis = app.visible_playlist_names();
                    vis.get(app.playlist_sel).map(|s| s.to_string())
                };
                app.filter_mode = false;
                app.playlist_filter.clear();
                app.start_download(only);
            }
            Char(c) => {
                app.playlist_filter.push(c);
                app.playlist_sel = 0;
            }
            _ => {}
        }
        return;
    }

    // ── Normal mode ──────────────────────────────────────────────────────────
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
        Char('/') => {
            // Enter filter mode
            app.filter_mode = true;
            app.playlist_filter.clear();
            app.playlist_sel = 0;
        }
        Char('?') => {
            app.show_help = true;
        }
        Char('l') => {
            save_log_to_file(app);
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns true for .NET exception-spam lines produced by sldl that are
/// only useful in verbose mode.
fn is_log_noise(s: &str) -> bool {
    let body = match s.strip_prefix("[sockseek] ") {
        Some(b) => b,
        None    => return false,
    };
    body.contains("Unobserved task exception")
        || body.contains("AggregateException:")
        || body.starts_with("System.")
        || body.starts_with("   at ")
        || body.starts_with("  ---> ")
        || body.starts_with("--- End of inner exception")
}

/// Scan `music_root` to determine which playlists already have audio on disk.
fn scan_statuses(
    music_root: &std::path::Path,
    names: &[String],
) -> HashMap<String, PlaylistStatus> {
    let mut map = HashMap::new();
    for name in names {
        let dir = music_root.join(name);
        let status = if !dir.exists() {
            PlaylistStatus::Unknown
        } else {
            let has_audio = std::fs::read_dir(&dir)
                .map(|rd| rd.filter_map(|e| e.ok()).any(|e| {
                    let n = e.file_name();
                    let ext = std::path::Path::new(&n)
                        .extension()
                        .and_then(|x| x.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    matches!(
                        ext.as_str(),
                        "mp3" | "flac" | "ogg" | "m4a" | "wav" | "opus" | "aac"
                    )
                }))
                .unwrap_or(false);
            if has_audio { PlaylistStatus::HasFiles } else { PlaylistStatus::Empty }
        };
        map.insert(name.clone(), status);
    }
    map
}

/// Write the current log buffer to `music_root/s2o_log_<timestamp>.txt`.
fn write_log(
    music_root: &std::path::Path,
    logs: &VecDeque<LogLine>,
) -> anyhow::Result<std::path::PathBuf> {
    use std::fmt::Write as _;
    let ts   = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = music_root.join(format!("s2o_log_{}.txt", ts));
    let mut content = String::new();
    for line in logs {
        let _ = writeln!(content, "[{}] {}", line.ts, line.text);
    }
    std::fs::write(&path, &content)?;
    Ok(path)
}

/// Save log to file and report success/failure back into the TUI log.
fn save_log_to_file(app: &mut App) {
    match write_log(&app.cfg.paths.music_root, &app.logs) {
        Ok(path) => app.push_log(format!("  ✓ Log saved → {}", path.display())),
        Err(e)   => app.push_log(format!("  ✗ Log save failed: {}", e)),
    }
}

// ── Public façade for main.rs ─────────────────────────────────────────────────

pub async fn run() -> Result<()> {
    let cfg = crate::config::load()?;
    run_tui(cfg).await
}
