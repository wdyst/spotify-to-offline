/// Download orchestration.
///
/// Runs playlists concurrently up to `concurrent_playlists`.
/// For each playlist, tries providers in configured order.
/// Outcomes are written to the SQLite DB, tags are normalised,
/// and events are streamed to the TUI (or stdout for CLI runs).

use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, Semaphore};

use crate::{
    config::Config,
    db::{self, Record, Stats, Status},
    import::{self, TrackRow},
    notify,
    providers::{self, TrackOutcome},
    tags,
};

// ── TUI event types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Event {
    Log(String),
    PlaylistStart { name: String, index: usize, total: usize },
    TrackDone     { artist: String, title: String, status: String },
    PlaylistDone  { name: String },
    AllDone       { stats: Stats },
    /// Emitted by an import task when it completes
    ImportDone    { count: usize },
    /// Emitted by an M3U generation task when it completes
    M3uDone,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// CLI entry point — no event channel, progress goes to stdout via the
/// provider log callbacks.
pub async fn run_all_cli(cfg: &Config, only: Option<&str>) -> Result<()> {
    run_all(cfg, only, None).await
}

/// Main orchestration. Pass `Some(sender)` when running under the TUI
/// to stream live events; `None` for CLI runs.
pub async fn run_all(
    cfg:      &Config,
    only:     Option<&str>,
    event_tx: Option<UnboundedSender<Event>>,
) -> Result<()> {
    let db   = Arc::new(std::sync::Mutex::new(db::open()?));
    let t0   = Utc::now();

    let all_playlists = import::list_playlists()?;
    if all_playlists.is_empty() {
        log(&event_tx, "No playlists found — run `s2o import` first.");
        return Ok(());
    }

    let playlists: Vec<_> = all_playlists.iter()
        .filter(|p| match only {
            Some(name) => p.file_stem().and_then(|s| s.to_str()) == Some(name),
            None       => true,
        })
        .cloned()
        .collect();

    let total = playlists.len();
    log(&event_tx, &format!("Starting {} playlist(s)…", total));

    let sem       = Arc::new(Semaphore::new(cfg.download.concurrent_playlists.max(1)));
    let providers = Arc::new(providers::build_providers(cfg));
    let cfg_arc   = Arc::new(cfg.clone());

    let handles: Vec<_> = playlists.into_iter().enumerate().map(|(idx, csv_path)| {
        let sem       = Arc::clone(&sem);
        let providers = Arc::clone(&providers);
        let cfg       = Arc::clone(&cfg_arc);
        let db        = Arc::clone(&db);
        let event_tx  = event_tx.clone();

        let name = csv_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            emit(&event_tx, Event::PlaylistStart {
                name: name.clone(), index: idx + 1, total,
            });

            if let Err(e) = download_playlist(
                &cfg, &providers, &csv_path, &name, &db, &event_tx,
            ).await {
                log(&event_tx, &format!("  ✗ {}: {}", name, e));
            }

            emit(&event_tx, Event::PlaylistDone { name });
        })
    }).collect();

    for h in handles { let _ = h.await; }

    // Completion summary
    let stats = {
        let conn = db.lock().unwrap();
        db::stats_since(&conn, &t0).unwrap_or_default()
    };

    let summary = format!(
        "Done — {} found  {} not found  {} failed  {} quality warnings",
        stats.ok, stats.not_found, stats.failed, stats.quality_warns,
    );
    log(&event_tx, &format!("━━ {}", summary));
    emit(&event_tx, Event::AllDone { stats });

    if cfg_arc.notifications.enabled && cfg_arc.notifications.on_completion {
        notify::send("spotify-to-offline", &summary);
    }

    Ok(())
}

// ── Per-playlist ──────────────────────────────────────────────────────────────

async fn download_playlist(
    cfg:       &Config,
    providers: &[Box<dyn crate::providers::Provider>],
    csv_path:  &Path,
    name:      &str,
    db:        &Arc<std::sync::Mutex<rusqlite::Connection>>,
    event_tx:  &Option<UnboundedSender<Event>>,
) -> Result<()> {
    if providers.is_empty() {
        log(event_tx, "  ⚠ No providers configured — check Settings.");
        return Ok(());
    }

    let preferred = cfg.download.playlist_overrides
        .get(name)
        .cloned()
        .unwrap_or_else(|| cfg.download.preferred_format.clone());

    let output_dir = cfg.paths.music_root.clone();

    // Forward per-provider log lines into the main event stream
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let event_fwd = event_tx.clone();
    tokio::spawn(async move {
        while let Some(line) = log_rx.recv().await {
            log(&event_fwd, &line);
        }
    });

    // Try each provider; when fallback is disabled, stop after the first one.
    let mut outcomes: Vec<(TrackRow, TrackOutcome)> = Vec::new();

    for (i, provider) in providers.iter().enumerate() {
        match provider.download_playlist(csv_path, &output_dir, &log_tx).await {
            Err(e) => {
                log(event_tx, &format!("  ⚠ {} error: {}", provider.name(), e));
                continue;
            }
            Ok(results) => {
                if i == 0 {
                    outcomes = results;
                } else if cfg.provider.fallback_enabled {
                    // Merge: fill in NotFound slots from this provider's results
                    for (track, slot) in &mut outcomes {
                        if matches!(slot, TrackOutcome::NotFound) {
                            if let Some((_, fb)) = results.iter().find(|(t, _)| {
                                t.artist == track.artist && t.title == track.title
                            }) {
                                *slot = fb.clone();
                            }
                        }
                    }
                }
            }
        }

        let still_missing = outcomes.iter().any(|(_, o)| matches!(o, TrackOutcome::NotFound));
        if !cfg.provider.fallback_enabled || !still_missing { break; }
    }

    // Record outcomes, write tags, emit per-track events
    let primary_provider = providers.first().map(|p| p.name().to_string());
    let conn             = db.lock().unwrap();

    for (track, outcome) in &outcomes {
        process_track(
            &track,
            outcome,
            name,
            &preferred,
            primary_provider.as_deref(),
            &conn,
            event_tx,
            cfg,
        );
    }

    Ok(())
}

// ── Per-track recording ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn process_track(
    track:    &TrackRow,
    outcome:  &TrackOutcome,
    playlist: &str,
    wanted:   &str,
    provider: Option<&str>,
    conn:     &rusqlite::Connection,
    event_tx: &Option<UnboundedSender<Event>>,
    cfg:      &Config,
) {
    match outcome {
        TrackOutcome::Ok { format, path } => {
            let actual = format.to_lowercase();
            let want   = wanted.to_lowercase();

            // A lossless upgrade (mp3 → flac) is never a "downgrade".
            let is_downgrade = want != "any"
                && actual != want
                && !(actual == "flac" && (want == "mp3" || want == "any"));

            if is_downgrade && cfg.download.quality_warning {
                let msg = format!(
                    "  ⚠ Quality: {} — {} [got {} wanted {}]",
                    track.artist, track.title, actual, want,
                );
                log(event_tx, &msg);

                if cfg.notifications.enabled && cfg.notifications.on_quality_downgrade {
                    notify::send("s2o — quality warning", &msg);
                }

                emit(event_tx, Event::TrackDone {
                    artist: track.artist.clone(),
                    title:  track.title.clone(),
                    status: format!("quality_warn [{}→{}]", want, actual),
                });
                let _ = db::insert(conn, &Record {
                    playlist, artist: &track.artist, title: &track.title,
                    album: &track.album, provider,
                    status:    Status::QualityWarn { wanted: want, got: actual },
                    file_path: Some(path.as_str()),
                    format:    Some(format.as_str()),
                });
            } else {
                emit(event_tx, Event::TrackDone {
                    artist: track.artist.clone(),
                    title:  track.title.clone(),
                    status: format!("ok [{}]", actual),
                });
                let _ = db::insert(conn, &Record {
                    playlist, artist: &track.artist, title: &track.title,
                    album: &track.album, provider,
                    status:    Status::Ok,
                    file_path: Some(path.as_str()),
                    format:    Some(format.as_str()),
                });
            }

            // Normalise tags from the Exportify metadata
            if let Err(e) = tags::write(path, track) {
                log(event_tx, &format!("  · tag skipped ({})", e));
            }
        }

        TrackOutcome::NotFound => {
            emit(event_tx, Event::TrackDone {
                artist: track.artist.clone(),
                title:  track.title.clone(),
                status: "not found".into(),
            });
            let _ = db::insert(conn, &Record {
                playlist, artist: &track.artist, title: &track.title,
                album: &track.album, provider,
                status: Status::NotFound, file_path: None, format: None,
            });
        }

        TrackOutcome::Failed { reason } => {
            log(event_tx, &format!("  ✗ {} — {} ({})", track.artist, track.title, reason));
            emit(event_tx, Event::TrackDone {
                artist: track.artist.clone(),
                title:  track.title.clone(),
                status: format!("failed: {}", reason),
            });
            let _ = db::insert(conn, &Record {
                playlist, artist: &track.artist, title: &track.title,
                album: &track.album, provider,
                status: Status::Failed, file_path: None, format: None,
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn emit(tx: &Option<UnboundedSender<Event>>, ev: Event) {
    if let Some(tx) = tx { let _ = tx.send(ev); }
}

fn log(tx: &Option<UnboundedSender<Event>>, msg: &str) {
    emit(tx, Event::Log(msg.to_string()));
}
