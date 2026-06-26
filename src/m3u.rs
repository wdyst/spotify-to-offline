/// M3U playlist generation.
///
/// For each playlist CSV:
///   - Walk music_root looking for matching audio files (fuzzy match on artist + title)
///   - Write M3U using the configured DAP profile's path style
///
/// Fuzzy matching: strsim::jaro_winkler on "artist - title" strings.
/// Threshold tuned to catch minor differences in spacing/punctuation.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strsim::jaro_winkler;

use crate::config::Config;
use crate::dap::DapProfile;
use crate::import::{list_playlists, load_playlist};

const MATCH_THRESHOLD: f64 = 0.88;
const AUDIO_EXTS: &[&str]  = &["flac", "mp3", "opus", "ogg", "aac", "wav", "m4a"];

// ── Entry points ──────────────────────────────────────────────────────────────

/// CLI version — prints to stdout.
pub fn run(cfg: &Config, profile_name: Option<&str>) -> Result<()> {
    run_with_log(cfg, profile_name, |s| println!("{}", s))
}

/// TUI version — sends progress through a log callback.
pub fn run_with_log(
    cfg:          &Config,
    profile_name: Option<&str>,
    log:          impl Fn(String),
) -> Result<()> {
    let profile    = pick_profile(cfg, profile_name)?;
    let music_root = profile.music_root.as_deref().unwrap_or(&cfg.paths.music_root);
    let m3u_dir    = profile.m3u_dir.as_deref().unwrap_or(&cfg.paths.playlists_dir);

    std::fs::create_dir_all(m3u_dir)?;

    log(format!("Scanning {}…", music_root.display()));
    let library = scan_library(music_root)?;
    log(format!("  · {} audio files indexed", library.len()));

    let playlists = list_playlists()?;
    if playlists.is_empty() {
        log("  ⚠ No playlists imported yet — run Import first.".into());
        return Ok(());
    }

    for csv_path in &playlists {
        let name     = csv_path.file_stem().unwrap_or_default().to_string_lossy();
        let m3u_path = m3u_dir.join(format!("{}.m3u8", name));

        match generate_m3u(csv_path, &m3u_path, &library, profile, music_root, m3u_dir) {
            Ok((matched, total)) =>
                log(format!("  ✓ {} ({}/{} tracks)", name, matched, total)),
            Err(e) =>
                log(format!("  ✗ {}: {}", name, e)),
        }
    }
    Ok(())
}

// ── Generate one M3U ──────────────────────────────────────────────────────────

fn generate_m3u(
    csv_path:   &Path,
    m3u_path:   &Path,
    library:    &HashMap<String, PathBuf>,
    profile:    &DapProfile,
    music_root: &Path,
    m3u_dir:    &Path,
) -> Result<(usize, usize)> {
    let tracks = load_playlist(csv_path)
        .with_context(|| format!("Cannot read {}", csv_path.display()))?;

    let total = tracks.len();
    let mut lines: Vec<String> = Vec::new();

    if profile.extended { lines.push("#EXTM3U".into()); }

    let mut matched = 0;
    for track in &tracks {
        let key   = normalise(&format!("{} - {}", track.artist, track.title));
        let found = find_track(&key, library);

        if let Some(file_path) = found {
            if profile.extended {
                lines.push(format!(
                    "#EXTINF:{},{} - {}",
                    track.length, track.artist, track.title
                ));
            }
            lines.push(profile.format_path(file_path, m3u_dir, music_root));
            matched += 1;
        }
    }

    // Don't write an empty M3U
    if lines.is_empty() || (profile.extended && lines == vec!["#EXTM3U"]) {
        return Ok((0, total));
    }

    let content = if profile.utf8_bom {
        format!("\u{FEFF}{}\n", lines.join("\n"))
    } else {
        format!("{}\n", lines.join("\n"))
    };

    std::fs::write(m3u_path, content)?;
    Ok((matched, total))
}

// ── Library scan ──────────────────────────────────────────────────────────────

fn scan_library(root: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut map = HashMap::new();
    scan_dir(root, &mut map);
    Ok(map)
}

fn scan_dir(dir: &Path, map: &mut HashMap<String, PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e)  => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, map);
        } else if is_audio(&path) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let key = normalise(stem);
                map.entry(key).or_insert_with(|| path.clone());
            }
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ── Fuzzy match ───────────────────────────────────────────────────────────────

fn find_track<'a>(query: &str, library: &'a HashMap<String, PathBuf>) -> Option<&'a PathBuf> {
    if let Some(p) = library.get(query) { return Some(p); }

    library.iter()
        .map(|(key, path)| (jaro_winkler(query, key), path))
        .filter(|(score, _)| *score >= MATCH_THRESHOLD)
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .map(|(_, path)| path)
}

// ── Normalise ─────────────────────────────────────────────────────────────────

fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Profile picker ────────────────────────────────────────────────────────────

fn pick_profile<'a>(cfg: &'a Config, name: Option<&str>) -> Result<&'a DapProfile> {
    match name {
        Some(n) => cfg.dap_profiles.iter()
            .find(|p| p.name == n)
            .with_context(|| format!("DAP profile '{}' not found in config", n)),
        None    => cfg.dap_profiles.first()
            .context("No DAP profiles configured"),
    }
}
