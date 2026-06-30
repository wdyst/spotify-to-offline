/// Soulseek provider — wraps slsk-batchdl (sockseek / sldl) as a subprocess.
///
/// The whole playlist CSV is handed to sockseek in a single call. After it
/// finishes we read back the M3U it wrote, extract the actual file paths and
/// formats, and match them to track rows — so tag-writing and quality detection
/// work correctly, with no guessing based on position.
///
/// Compatible with slsk-batchdl v3.0.1+.  The binary can be named
/// `sockseek[.exe]` or `sldl[.exe]`; configure `sockseek_path` in Settings
/// if it lives somewhere other than next to s2o or on your PATH.

use anyhow::{bail, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use super::{Provider, TrackOutcome};
use crate::config::Config;
use crate::import::{load_playlist, TrackRow};

// ── Provider struct ───────────────────────────────────────────────────────────

pub struct SoulseekProvider {
    username: String,
    password: String,
    exe:      PathBuf,
}

impl SoulseekProvider {
    pub fn new(cfg: &Config) -> Self {
        let exe = cfg.paths.sockseek_path.clone().unwrap_or_else(find_sockseek_exe);
        Self {
            username: cfg.soulseek.username.clone(),
            password: cfg.soulseek.password.clone(),
            exe,
        }
    }
}

/// Look for the soulseek binary next to s2o first, then fall back to PATH.
/// Accepts both `sockseek` and `sldl` (slsk-batchdl) names.
fn find_sockseek_exe() -> PathBuf {
    let candidates = if cfg!(windows) {
        &["sockseek.exe", "sldl.exe"][..]
    } else {
        &["sockseek", "sldl"][..]
    };

    // Prefer a copy sitting next to the s2o binary itself
    if let Ok(bin) = std::env::current_exe() {
        if let Some(dir) = bin.parent() {
            for name in candidates {
                let p = dir.join(name);
                if p.exists() { return p; }
            }
        }
    }

    // Try PATH
    for name in candidates {
        if let Ok(p) = which::which(name) { return p; }
    }

    // Last resort: let the OS give a helpful "not found" error at runtime
    PathBuf::from("sockseek")
}

// ── Subprocess ────────────────────────────────────────────────────────────────

async fn run_sockseek(
    exe:        &Path,
    username:   &str,
    password:   &str,
    csv_path:   &Path,
    output_dir: &Path,
    m3u_path:   &Path,
    log_tx:     &UnboundedSender<String>,
) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.args([
        "--user",          username,
        "--pass",          password,
        "-p",              &output_dir.to_string_lossy(),
        "--skip-music-dir", &output_dir.to_string_lossy(),
        "--write-playlist",
        "--playlist-path", &m3u_path.to_string_lossy(),
        "--artist-col",    "artist",
        "--title-col",     "title",
        "--length-col",    "length",
        "--time-format",   "s",
    ]);
    cmd.arg(csv_path.to_string_lossy().as_ref());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;

    // Stream stdout / stderr into the TUI log as they arrive
    if let Some(out) = child.stdout.take() {
        let tx = log_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(l);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = log_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(format!("[sockseek] {}", l));
            }
        });
    }

    let status = child.wait().await?;

    // Exit code 1 means some tracks weren't found — expected and non-fatal.
    // Anything higher is a real error (bad credentials, binary crash, etc.).
    if status.code().unwrap_or(0) > 1 {
        bail!(
            "sockseek exited with code {} (bad credentials, connection failure, or bad args)",
            status.code().unwrap_or(-1),
        );
    }

    Ok(())
}

// ── M3U parsing ───────────────────────────────────────────────────────────────

/// Read the M3U sockseek produced and return (title_lowercase, absolute_path) pairs.
///
/// We use these to match back to TrackRows instead of assuming track order,
/// which means correct per-file paths flow through to tag writing and format
/// detection.
fn parse_sockseek_m3u(m3u_path: &Path, output_dir: &Path) -> Vec<(String, PathBuf)> {
    let text = match std::fs::read_to_string(m3u_path) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };

    let mut pairs         = Vec::new();
    let mut pending_title = String::new();

    for raw in text.lines() {
        let line = raw.trim();

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            // Format: #EXTINF:seconds,Artist - Title
            pending_title = rest
                .splitn(2, ',')
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_lowercase();
        } else if !line.is_empty() && !line.starts_with('#') {
            // This line is the file path
            let sep = std::path::MAIN_SEPARATOR_STR;
            let normalised = line.replace('/', sep).replace('\\', sep);

            let path = if Path::new(&normalised).is_absolute() {
                PathBuf::from(&normalised)
            } else {
                output_dir.join(&normalised)
            };

            if !pending_title.is_empty() {
                pairs.push((std::mem::take(&mut pending_title), path));
            }
        }
    }

    pairs
}

/// Map every TrackRow to an outcome by looking it up in the parsed M3U.
///
/// Matching strategy:
///   1. Exact  "artist - title" match against the EXTINF display string
///   2. Substring: EXTINF string contains the track title
///
/// If neither matches (or the resolved file doesn't exist on disk), the
/// track is marked NotFound so a fallback provider can pick it up.
fn match_tracks_to_files(
    tracks:     &[TrackRow],
    m3u_path:   &Path,
    output_dir: &Path,
) -> Vec<(TrackRow, TrackOutcome)> {
    let found = parse_sockseek_m3u(m3u_path, output_dir);

    tracks.iter().map(|track| {
        let full_key   = format!("{} - {}", track.artist, track.title).to_lowercase();
        let title_key  = track.title.to_lowercase();

        let hit = found.iter().find(|(display, path)| {
            path.exists()
                && (display == &full_key || display.contains(&title_key))
        });

        let outcome = match hit {
            Some((_, path)) => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("flac")
                    .to_lowercase();
                TrackOutcome::Ok {
                    path:   path.to_string_lossy().into_owned(),
                    format: ext,
                }
            }
            None => TrackOutcome::NotFound,
        };

        (track.clone(), outcome)
    }).collect()
}

// ── Provider impl ─────────────────────────────────────────────────────────────

#[async_trait]
impl Provider for SoulseekProvider {
    fn name(&self) -> &'static str { "soulseek" }

    /// Download a single track by writing a one-row CSV and delegating to
    /// the same sockseek + M3U flow used for full playlists.
    async fn download_track(
        &self,
        track:      &TrackRow,
        output_dir: &Path,
    ) -> Result<TrackOutcome> {
        let tmp_csv = output_dir.join(".s2o_single.csv");
        let tmp_m3u = output_dir.join(".s2o_single.m3u8");

        {
            let mut wtr = csv::WriterBuilder::new()
                .has_headers(true)
                .from_path(&tmp_csv)?;
            wtr.serialize(track)?;
            wtr.flush()?;
        }

        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let run = run_sockseek(
            &self.exe, &self.username, &self.password,
            &tmp_csv, output_dir, &tmp_m3u, &tx,
        ).await;

        let _ = std::fs::remove_file(&tmp_csv);
        run?;

        let mut outcomes = match_tracks_to_files(&[track.clone()], &tmp_m3u, output_dir);
        let _ = std::fs::remove_file(&tmp_m3u);

        Ok(outcomes.pop().map(|(_, o)| o).unwrap_or(TrackOutcome::NotFound))
    }

    /// Download an entire playlist. Passes the full CSV to sockseek, then
    /// reads the output M3U to determine exactly what was found and where.
    async fn download_playlist(
        &self,
        csv_path:   &Path,
        output_dir: &Path,
        log_tx:     &UnboundedSender<String>,
    ) -> Result<Vec<(TrackRow, TrackOutcome)>> {
        std::fs::create_dir_all(output_dir)?;

        let stem = csv_path.file_stem().unwrap_or_default().to_string_lossy();
        let m3u  = output_dir.join(format!("{}.m3u8", stem));

        run_sockseek(
            &self.exe, &self.username, &self.password,
            csv_path, output_dir, &m3u, log_tx,
        ).await?;

        let tracks   = load_playlist(csv_path).unwrap_or_default();
        let outcomes = match_tracks_to_files(&tracks, &m3u, output_dir);

        let found = outcomes.iter().filter(|(_, o)| matches!(o, TrackOutcome::Ok { .. })).count();
        let icon  = if found == tracks.len() { "✓" } else { "·" };
        let _ = log_tx.send(format!("  {} Soulseek: {}/{} tracks found", icon, found, tracks.len()));

        Ok(outcomes)
    }
}
