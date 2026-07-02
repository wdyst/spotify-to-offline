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
use tokio::sync::{mpsc::UnboundedSender, watch, Semaphore};

use crate::{
    config::Config,
    db::{self, Record, Stats, Status},
    import::{self, TrackRow},
    notify,
    providers::{self, ProviderEvent, TerminalKind, TrackOutcome},
    tags,
};

// ── TUI event types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Event {
    Log(String),
    PlaylistStart { name: String, index: usize, total: usize },
    /// Live per-playlist progress while tracks stream in.
    PlaylistProgress {
        name:    String,
        done:    usize,
        total:   usize,
        ok:      usize,
        failed:  usize,
        skipped: usize,
    },
    TrackDone     { artist: String, title: String, status: String },
    PlaylistDone  { name: String },
    AllDone       { stats: Stats },
    /// Emitted by an import task when it completes
    ImportDone    { count: usize },
    /// Emitted by an M3U generation task when it completes
    M3uDone,
    /// Emitted when the sldl auto-download finishes
    SldlDone { ok: bool },
}

// ── Public API ────────────────────────────────────────────────────────────────

/// CLI entry point — no event channel, progress goes to stdout via the
/// provider log callbacks.
pub async fn run_all_cli(cfg: &Config, only: Option<&str>) -> Result<()> {
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    run_all(cfg, only, None, cancel_rx).await
}

/// Main orchestration. Pass `Some(sender)` when running under the TUI
/// to stream live events; `None` for CLI runs. Flip `cancel` to true to
/// abort: running subprocesses are killed, queued playlists are skipped.
pub async fn run_all(
    cfg:      &Config,
    only:     Option<&str>,
    event_tx: Option<UnboundedSender<Event>>,
    cancel:   watch::Receiver<bool>,
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
        let cancel    = cancel.clone();

        let name = csv_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            if *cancel.borrow() {
                log(&event_tx, &format!("  ⊘ {} skipped (cancelled)", name));
                return;
            }

            emit(&event_tx, Event::PlaylistStart {
                name: name.clone(), index: idx + 1, total,
            });

            if let Err(e) = download_playlist(
                &cfg, &providers, &csv_path, &name, &db, &event_tx, cancel,
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
        "Done — {} downloaded  {} reused  {} not found  {} failed  {} quality warnings",
        stats.ok, stats.skipped, stats.not_found, stats.failed, stats.quality_warns,
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
    cancel:    watch::Receiver<bool>,
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

    // Translate provider events into TUI events, maintaining live counters.
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<ProviderEvent>();
    let event_fwd = event_tx.clone();
    let pl_name   = name.to_string();
    tokio::spawn(async move {
        let (mut done, mut total)              = (0usize, 0usize);
        let (mut ok, mut failed, mut skipped)  = (0usize, 0usize, 0usize);

        while let Some(ev) = ev_rx.recv().await {
            match ev {
                ProviderEvent::Log(l) => log(&event_fwd, &l),

                ProviderEvent::TrackList { total: t, existing } => {
                    // New provider run — reset live counters
                    total = t;
                    done = 0; ok = 0; failed = 0; skipped = 0;
                    if existing > 0 {
                        log(&event_fwd, &format!(
                            "  · {}: {}/{} tracks already on disk — reusing",
                            pl_name, existing, t,
                        ));
                    }
                    emit(&event_fwd, Event::PlaylistProgress {
                        name: pl_name.clone(), done, total, ok, failed, skipped,
                    });
                }

                ProviderEvent::TrackFinished { outcome, .. } => {
                    done += 1;
                    match outcome {
                        TerminalKind::Succeeded => ok      += 1,
                        TerminalKind::Skipped   => skipped += 1,
                        TerminalKind::Failed    => failed  += 1,
                    }
                    emit(&event_fwd, Event::PlaylistProgress {
                        name: pl_name.clone(), done, total: total.max(done),
                        ok, failed, skipped,
                    });
                }
            }
        }
    });

    // Try each provider; when fallback is disabled, stop after the first one.
    let mut outcomes: Vec<(TrackRow, TrackOutcome)> = Vec::new();

    for (i, provider) in providers.iter().enumerate() {
        match provider.download_playlist(csv_path, &output_dir, &ev_tx, cancel.clone()).await {
            Err(e) => {
                let msg = e.to_string();
                log(event_tx, &format!("  ⚠ {} error: {}", provider.name(), msg));
                // Hint the user toward auto-install when the binary is simply missing
                let lower = msg.to_lowercase();
                if lower.contains("not found") || lower.contains("no such file")
                    || lower.contains("cannot find") || lower.contains("os error 2")
                {
                    log(event_tx,
                        "    → sldl not installed — press [s] › scroll to 'Download sldl' › Enter");
                }
                if lower.contains("cancelled") { break; }
                continue;
            }
            Ok(results) => {
                if i == 0 || outcomes.is_empty() {
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
        if !cfg.provider.fallback_enabled || !still_missing || *cancel.borrow() { break; }
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

        TrackOutcome::Skipped { path, format } => {
            // Already on disk — record the reuse, but don't re-tag the file.
            emit(event_tx, Event::TrackDone {
                artist: track.artist.clone(),
                title:  track.title.clone(),
                status: "skipped (already on disk)".into(),
            });
            let _ = db::insert(conn, &Record {
                playlist, artist: &track.artist, title: &track.title,
                album: &track.album, provider,
                status:    Status::Skipped,
                file_path: path.as_deref(),
                format:    format.as_deref(),
            });
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
    match tx {
        Some(tx) => { let _ = tx.send(ev); }
        // CLI run — no TUI listening, print the interesting events instead
        None => match ev {
            Event::Log(s) => println!("{}", s),
            Event::TrackDone { artist, title, status } =>
                println!("  {} — {} [{}]", artist, title, status),
            Event::PlaylistStart { name, index, total } =>
                println!("[{}/{}] ▶ {}", index, total, name),
            _ => {}
        }
    }
}

fn log(tx: &Option<UnboundedSender<Event>>, msg: &str) {
    emit(tx, Event::Log(msg.to_string()));
}
