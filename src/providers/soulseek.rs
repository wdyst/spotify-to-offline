/// Soulseek provider — wraps slsk-batchdl (sockseek / sldl) as a subprocess.
///
/// The whole playlist CSV is handed to sockseek in a single call with
/// `--progress-json`. Per-track outcomes come from the JSON event stream
/// (`track_state` / `track_list` events) and are cross-checked against the
/// `_index.csv` sockseek writes next to the downloads — so tag-writing,
/// quality detection and skip-reuse all see real file paths.
///
/// Compatible with slsk-batchdl v3.0.1+.  The binary can be named
/// `sockseek[.exe]` or `sldl[.exe]`; configure `sockseek_path` in Settings
/// if it lives somewhere other than next to s2o or on your PATH.

use anyhow::{bail, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;

use super::{Provider, ProviderEvent, TerminalKind, TrackOutcome};
use crate::config::Config;
use crate::import::{load_playlist, TrackRow};

// ── Provider struct ───────────────────────────────────────────────────────────

pub struct SoulseekProvider {
    username:    String,
    password:    String,
    exe:         PathBuf,
    pref_format: String,
    name_format: String,
}

impl SoulseekProvider {
    pub fn new(cfg: &Config) -> Self {
        let exe = cfg.paths.sockseek_path.clone().unwrap_or_else(find_sockseek_exe);
        Self {
            username:    cfg.soulseek.username.clone(),
            password:    cfg.soulseek.password.clone(),
            exe,
            pref_format: cfg.download.preferred_format.clone(),
            name_format: cfg.download.name_format.clone(),
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

// ── Per-track result parsed from the JSON event stream ───────────────────────

#[derive(Debug, Clone, Default)]
struct JsonOutcome {
    kind:           Option<TerminalKind>,
    download_path:  Option<String>,
    failure_reason: Option<String>,
}

// ── Subprocess ────────────────────────────────────────────────────────────────

async fn run_sockseek(
    exe:         &Path,
    username:    &str,
    password:    &str,
    csv_path:    &Path,
    output_dir:  &Path,
    pref_format: &str,
    name_format: &str,
    ev_tx:       &UnboundedSender<ProviderEvent>,
    mut cancel:  watch::Receiver<bool>,
) -> Result<HashMap<String, JsonOutcome>> {
    let mut cmd = Command::new(exe);
    cmd.args([
        "--user",           username,
        "--pass",           password,
        "-p",               &output_dir.to_string_lossy(),
        "--skip-music-dir", &output_dir.to_string_lossy(),
        "--progress-json",
        "--no-progress",
        "--artist-col",     "artist",
        "--title-col",      "title",
        "--length-col",     "length",
        "--time-format",    "s",
    ]);
    if !name_format.trim().is_empty() {
        cmd.args(["--name-format", name_format]);
    }
    // Soft format preference — sldl defaults to preferring mp3 otherwise.
    match pref_format.to_lowercase().as_str() {
        "any" | "" => {}
        fmt        => { cmd.args(["--pref-format", fmt]); }
    }
    cmd.arg(csv_path.to_string_lossy().as_ref());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn()?;

    // stdout: JSON progress events + human-readable job lines
    let json_task = child.stdout.take().map(|out| {
        let tx = ev_tx.clone();
        tokio::spawn(async move {
            let mut outcomes: HashMap<String, JsonOutcome> = HashMap::new();
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let trimmed = l.trim();
                if trimmed.starts_with('{') {
                    handle_json_line(trimmed, &mut outcomes, &tx);
                } else if !trimmed.is_empty() {
                    let _ = tx.send(ProviderEvent::Log(l));
                }
            }
            outcomes
        })
    });
    if let Some(err) = child.stderr.take() {
        let tx = ev_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(ProviderEvent::Log(format!("[sockseek] {}", l)));
            }
        });
    }

    // Wait for exit, or kill on cancel
    let status = tokio::select! {
        st = child.wait() => st?,
        _ = wait_cancelled(&mut cancel) => {
            let _ = child.kill().await;
            // Drain whatever the reader collected before the kill
            if let Some(t) = json_task { let _ = t.await; }
            bail!("cancelled by user");
        }
    };

    let outcomes = match json_task {
        Some(t) => t.await.unwrap_or_default(),
        None    => HashMap::new(),
    };

    // Exit code 1 means some tracks weren't found — expected and non-fatal.
    // Anything higher is a real error (bad credentials, binary crash, etc.).
    if status.code().unwrap_or(0) > 1 {
        bail!(
            "sockseek exited with code {} (bad credentials, connection failure, or outdated sldl — \
             press [s] › 'Download sldl' to update)",
            status.code().unwrap_or(-1),
        );
    }

    Ok(outcomes)
}

async fn wait_cancelled(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow() { return; }
        if cancel.changed().await.is_err() {
            // Sender dropped without cancelling — never resolve
            std::future::pending::<()>().await;
        }
    }
}

/// Parse one JSON progress event and fold it into the outcome map.
fn handle_json_line(
    line:     &str,
    outcomes: &mut HashMap<String, JsonOutcome>,
    tx:       &UnboundedSender<ProviderEvent>,
) {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v)  => v,
        Err(_) => return,
    };
    let data = &v["data"];

    match v["type"].as_str().unwrap_or("") {
        // Emitted at run start; skipped-as-existing tracks only ever appear here.
        "track_list" => {
            let total    = data["total"].as_u64().unwrap_or(0) as usize;
            let existing = data["existing"].as_u64().unwrap_or(0) as usize;
            let _ = tx.send(ProviderEvent::TrackList { total, existing });

            if let Some(tracks) = data["tracks"].as_array() {
                for t in tracks {
                    let title = t["title"].as_str().unwrap_or("").to_lowercase();
                    if title.is_empty() { continue; }
                    let kind = match t["terminalOutcome"].as_str().unwrap_or("None") {
                        "Succeeded" => Some(TerminalKind::Succeeded),
                        "Skipped"   => Some(TerminalKind::Skipped),
                        "Failed"    => Some(TerminalKind::Failed),
                        _           => None,
                    };
                    if let Some(kind) = kind {
                        let entry = outcomes.entry(title).or_default();
                        entry.kind = Some(kind);
                        let _ = tx.send(ProviderEvent::TrackFinished { outcome: kind });
                    }
                }
            }
        }

        "track_state" => {
            let title = data["title"].as_str().unwrap_or("").to_lowercase();
            if title.is_empty() { return; }
            if data["lifecycleState"].as_str() != Some("Terminal") { return; }

            let kind = match data["terminalOutcome"].as_str().unwrap_or("") {
                "Succeeded" => TerminalKind::Succeeded,
                "Skipped"   => TerminalKind::Skipped,
                _           => TerminalKind::Failed,
            };
            let entry = outcomes.entry(title.clone()).or_default();
            entry.kind = Some(kind);
            if let Some(p) = data["downloadPath"].as_str() {
                entry.download_path = Some(p.to_string());
            }
            if let Some(r) = data["failureReason"].as_str() {
                entry.failure_reason = Some(r.to_string());
            }
            let _ = tx.send(ProviderEvent::TrackFinished { outcome: kind });
        }

        _ => {} // search_start / download_start / download_progress — ignored
    }
}

// ── sockseek index parsing ────────────────────────────────────────────────────

/// sockseek writes `<output_dir>/<playlist>/_index.csv` with the real path and
/// state of every track it has ever handled for that playlist.
/// States: 1 = Downloaded, 3 = AlreadyExists. Rows may repeat across runs —
/// later rows win.
pub fn parse_index(index_path: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let mut rdr = match csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(index_path)
    {
        Ok(r)  => r,
        Err(_) => return map,
    };

    for rec in rdr.records().flatten() {
        let filepath = rec.get(0).unwrap_or("").trim();
        let title    = rec.get(3).unwrap_or("").trim().to_lowercase();
        let state    = rec.get(6).unwrap_or("").trim();
        if filepath.is_empty() || title.is_empty() { continue; }
        if state == "1" || state == "3" {
            let sep  = std::path::MAIN_SEPARATOR_STR;
            let norm = filepath.replace('/', sep).replace('\\', sep);
            map.insert(title, PathBuf::from(norm));
        }
    }
    map
}

/// Map every TrackRow to an outcome using the JSON event stream, with the
/// on-disk index as backup for paths (skipped tracks carry no downloadPath).
fn resolve_outcomes(
    tracks:   &[TrackRow],
    json:     &HashMap<String, JsonOutcome>,
    index:    &HashMap<String, PathBuf>,
) -> Vec<(TrackRow, TrackOutcome)> {
    tracks.iter().map(|track| {
        let key = track.title.to_lowercase();

        let path_from_index = || index.get(&key).filter(|p| p.exists()).cloned();

        let outcome = match json.get(&key) {
            Some(j) => match j.kind {
                Some(TerminalKind::Succeeded) => {
                    let path = j.download_path.as_ref()
                        .map(PathBuf::from)
                        .filter(|p| p.exists())
                        .or_else(path_from_index);
                    match path {
                        Some(p) => TrackOutcome::Ok {
                            format: ext_of(&p),
                            path:   p.to_string_lossy().into_owned(),
                        },
                        None => TrackOutcome::NotFound,
                    }
                }
                Some(TerminalKind::Skipped) => {
                    let path = path_from_index();
                    TrackOutcome::Skipped {
                        format: path.as_ref().map(|p| ext_of(p)),
                        path:   path.map(|p| p.to_string_lossy().into_owned()),
                    }
                }
                Some(TerminalKind::Failed) => {
                    match j.failure_reason.as_deref() {
                        Some("NoSearchResults") | None => TrackOutcome::NotFound,
                        Some(r) => TrackOutcome::Failed { reason: r.to_string() },
                    }
                }
                None => TrackOutcome::NotFound,
            },
            // No JSON event for this track — trust the index if it has it
            None => match path_from_index() {
                Some(p) => TrackOutcome::Ok {
                    format: ext_of(&p),
                    path:   p.to_string_lossy().into_owned(),
                },
                None => TrackOutcome::NotFound,
            },
        };

        (track.clone(), outcome)
    }).collect()
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

// ── Provider impl ─────────────────────────────────────────────────────────────

#[async_trait]
impl Provider for SoulseekProvider {
    fn name(&self) -> &'static str { "soulseek" }

    /// Download a single track by writing a one-row CSV and delegating to
    /// the same sockseek + index flow used for full playlists.
    async fn download_track(
        &self,
        track:      &TrackRow,
        output_dir: &Path,
    ) -> Result<TrackOutcome> {
        let tmp_csv = output_dir.join(".s2o_single.csv");
        {
            let mut wtr = csv::WriterBuilder::new()
                .has_headers(true)
                .from_path(&tmp_csv)?;
            wtr.serialize(track)?;
            wtr.flush()?;
        }

        let (tx, _rx)   = tokio::sync::mpsc::unbounded_channel();
        let (_ctx, crx) = watch::channel(false);
        let run = run_sockseek(
            &self.exe, &self.username, &self.password,
            &tmp_csv, output_dir, &self.pref_format, &self.name_format,
            &tx, crx,
        ).await;

        let _ = std::fs::remove_file(&tmp_csv);
        let json = run?;

        let index = parse_index(&output_dir.join(".s2o_single").join("_index.csv"));
        let mut outcomes = resolve_outcomes(&[track.clone()], &json, &index);
        let _ = std::fs::remove_dir_all(output_dir.join(".s2o_single"));

        Ok(outcomes.pop().map(|(_, o)| o).unwrap_or(TrackOutcome::NotFound))
    }

    /// Download an entire playlist. Passes the full CSV to sockseek, then
    /// resolves per-track outcomes from the JSON events + on-disk index.
    async fn download_playlist(
        &self,
        csv_path:   &Path,
        output_dir: &Path,
        ev_tx:      &UnboundedSender<ProviderEvent>,
        cancel:     watch::Receiver<bool>,
    ) -> Result<Vec<(TrackRow, TrackOutcome)>> {
        std::fs::create_dir_all(output_dir)?;

        let json = run_sockseek(
            &self.exe, &self.username, &self.password,
            csv_path, output_dir, &self.pref_format, &self.name_format,
            ev_tx, cancel,
        ).await?;

        let stem  = csv_path.file_stem().unwrap_or_default().to_string_lossy();
        let index = parse_index(&output_dir.join(stem.as_ref()).join("_index.csv"));

        let tracks   = load_playlist(csv_path).unwrap_or_default();
        let outcomes = resolve_outcomes(&tracks, &json, &index);

        let found = outcomes.iter().filter(|(_, o)| {
            matches!(o, TrackOutcome::Ok { .. } | TrackOutcome::Skipped { .. })
        }).count();
        let icon = if found == tracks.len() { "✓" } else { "·" };
        let _ = ev_tx.send(ProviderEvent::Log(format!(
            "  {} Soulseek: {}/{} tracks on disk", icon, found, tracks.len(),
        )));

        Ok(outcomes)
    }
}
