use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;

use crate::dap::{default_profiles, DapProfile};

// ── Paths ─────────────────────────────────────────────────────────────────────

/// Config file. Portable: if config.toml sits next to the binary, use it.
/// Otherwise platform config dir (~/.config/s2o/ or %APPDATA%\s2o\).
pub fn config_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("config.toml");
            if p.exists() { return p; }
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("s2o")
        .join("config.toml")
}

/// Directory that holds config, DB, and working CSV folders.
pub fn data_dir() -> PathBuf {
    config_path().parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn raw_dir()  -> PathBuf { data_dir().join("playlists_raw")  }
pub fn work_dir() -> PathBuf { data_dir().join("playlists_work") }

// ── Config structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub soulseek:      SoulseekConfig,
    pub paths:         PathsConfig,
    pub provider:      ProviderConfig,
    pub download:      DownloadConfig,
    pub notifications: NotifyConfig,
    #[serde(default = "default_profiles")]
    pub dap_profiles:  Vec<DapProfile>,
    /// Show raw sldl output instead of filtered friendly lines.
    #[serde(default)]
    pub verbose_logs:  bool,
    /// Automatically write a log file to music_root on exit.
    #[serde(default)]
    pub auto_save_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoulseekConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub music_root:    PathBuf,
    pub playlists_dir: PathBuf,
    /// None = auto-detect next to binary or on PATH
    pub sockseek_path: Option<PathBuf>,
    pub ytdlp_path:    String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Ordered list: "soulseek" | "ytdlp"
    pub order:            Vec<String>,
    pub fallback_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub concurrent_playlists: usize,
    pub concurrent_tracks:    usize,
    /// "flac" | "mp3" | "any"
    pub preferred_format:     String,
    /// sldl --name-format: how downloaded files are named/organised.
    /// Default sorts into Artist/Album folders using the CSV metadata.
    #[serde(default = "default_name_format")]
    pub name_format:          String,
    pub quality_warning:      bool,
    /// Per-playlist format overrides: { "playlist name" => "mp3" }
    #[serde(default)]
    pub playlist_overrides: std::collections::HashMap<String, String>,
}

pub fn default_name_format() -> String {
    "{sartist(/)salbum(/)stitle|sartist(/)stitle|filename}".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    pub enabled:              bool,
    pub on_completion:        bool,
    pub on_quality_downgrade: bool,
}

impl Default for Config {
    fn default() -> Self {
        let music = dirs::audio_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Music"));
        Config {
            soulseek: SoulseekConfig::default(),
            paths: PathsConfig {
                music_root:    music.clone(),
                playlists_dir: music.join("Playlists"),
                sockseek_path: None,
                ytdlp_path:    "yt-dlp".into(),
            },
            provider: ProviderConfig {
                order:            vec!["soulseek".into()],
                fallback_enabled: false,
            },
            download: DownloadConfig {
                concurrent_playlists: 2,
                concurrent_tracks:    4,
                preferred_format:     "flac".into(),
                name_format:          default_name_format(),
                quality_warning:      true,
                playlist_overrides:   Default::default(),
            },
            notifications: NotifyConfig {
                enabled:              true,
                on_completion:        true,
                on_quality_downgrade: true,
            },
            dap_profiles:  default_profiles(),
            verbose_logs:  false,
            auto_save_log: false,
        }
    }
}

// ── Load / save ───────────────────────────────────────────────────────────────

pub fn load() -> Result<Config> {
    let path = config_path();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read config: {}", path.display()))?;
    toml::from_str(&text).context("Config parse error — run `s2o setup` to reconfigure")
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    std::fs::write(&path, toml::to_string_pretty(cfg)?)?;
    Ok(())
}

pub fn show() -> Result<()> {
    let cfg = load()?;
    println!("Config:      {}", config_path().display());
    println!("Music root:  {}", cfg.paths.music_root.display());
    println!("Playlists:   {}", cfg.paths.playlists_dir.display());
    println!("Providers:   {}", cfg.provider.order.join(" → "));
    println!("Fallback:    {}", cfg.provider.fallback_enabled);
    println!("Format:      {}", cfg.download.preferred_format);
    println!("Concurrency: {} playlists / {} tracks",
        cfg.download.concurrent_playlists, cfg.download.concurrent_tracks);
    println!("Notifs:      {}", cfg.notifications.enabled);
    println!("DAP profile: {}", cfg.dap_profiles.first().map(|p| p.name.as_str()).unwrap_or("none"));
    Ok(())
}

// ── Setup wizard ──────────────────────────────────────────────────────────────

fn ask(label: &str, default: &str) -> Result<String> {
    print!("  {label} [{default}]: ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    let v = buf.trim().to_string();
    Ok(if v.is_empty() { default.to_string() } else { v })
}

fn ask_bool(label: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    print!("  {label} [{hint}]: ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(match buf.trim().to_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no"  => false,
        _            => default,
    })
}

pub fn run_setup() -> Result<()> {
    println!("╔═══════════════════════════════════════════════╗");
    println!("║   spotify-to-offline  ·  First-Run Setup     ║");
    println!("╚═══════════════════════════════════════════════╝");
    println!("  Press Enter to accept defaults.\n");

    let mut cfg = Config::default();

    // ── Paths ─────────────────────────────────────────────────────────────
    println!("── Paths ──────────────────────────────────────────");
    let music_def = cfg.paths.music_root.display().to_string();
    let music     = PathBuf::from(ask("Music root directory", &music_def)?);
    let pl_def    = music.join("Playlists").display().to_string();
    let playlists = PathBuf::from(ask("Playlists directory", &pl_def)?);
    cfg.paths.music_root    = music;
    cfg.paths.playlists_dir = playlists;

    // ── Provider ──────────────────────────────────────────────────────────
    println!("\n── Download Provider ──────────────────────────────");
    println!("  soulseek  — best quality, free account at slsknet.org");
    println!("  ytdlp     — no login required, YouTube source");
    println!("  both      — soulseek first, yt-dlp fills gaps");
    let prov = ask("Provider", "soulseek")?;
    match prov.as_str() {
        "both" => {
            cfg.provider.order            = vec!["soulseek".into(), "ytdlp".into()];
            cfg.provider.fallback_enabled = true;
        }
        "ytdlp" => cfg.provider.order = vec!["ytdlp".into()],
        _        => cfg.provider.order = vec!["soulseek".into()],
    }

    // Soulseek credentials
    if cfg.provider.order.contains(&"soulseek".to_string()) {
        println!("\n── Soulseek Credentials ───────────────────────────");
        cfg.soulseek.username = ask("  Username", "")?;
        print!("  Password: ");
        io::stdout().flush()?;
        let mut pw = String::new();
        io::stdin().read_line(&mut pw)?;
        cfg.soulseek.password = pw.trim().to_string();
        println!();
        if !cfg.provider.fallback_enabled {
            cfg.provider.fallback_enabled =
                ask_bool("Enable yt-dlp fallback for not-found tracks?", false)?;
            if cfg.provider.fallback_enabled
                && !cfg.provider.order.contains(&"ytdlp".to_string())
            {
                cfg.provider.order.push("ytdlp".into());
            }
        }
    }

    if cfg.provider.order.contains(&"ytdlp".to_string()) {
        println!("\n── yt-dlp Path ────────────────────────────────────");
        println!("  Install with: pip install yt-dlp");
        cfg.paths.ytdlp_path = ask("yt-dlp command or path", "yt-dlp")?;
    }

    // ── Download ──────────────────────────────────────────────────────────
    println!("\n── Download Settings ──────────────────────────────");
    cfg.download.concurrent_playlists =
        ask("Concurrent playlists", "2")?.parse().unwrap_or(2);
    cfg.download.preferred_format =
        ask("Preferred format  (flac / mp3 / any)", "flac")?;
    cfg.download.quality_warning =
        ask_bool("Log + notify when quality is lower than preferred?", true)?;

    // ── Notifications ─────────────────────────────────────────────────────
    println!("\n── Notifications ──────────────────────────────────");
    cfg.notifications.enabled = ask_bool("Enable system notifications?", true)?;
    if cfg.notifications.enabled {
        cfg.notifications.on_completion =
            ask_bool("Notify when a batch completes?", true)?;
        cfg.notifications.on_quality_downgrade =
            ask_bool("Notify on quality downgrade?", true)?;
    }

    // ── DAP profile ───────────────────────────────────────────────────────
    println!("\n── DAP Profile ────────────────────────────────────");
    let profiles = default_profiles();
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    println!("  Available: {}", names.join(", "));
    let chosen = ask("Default DAP profile", "universal")?;
    cfg.dap_profiles.sort_by_key(|p| if p.name == chosen { 0i32 } else { 1 });

    save(&cfg)?;
    println!("\n  ✓ Saved: {}", config_path().display());
    println!("  Run `s2o` to open the TUI, or use subcommands directly.");
    println!("  All settings editable any time via [s]ettings in the TUI.\n");
    Ok(())
}
