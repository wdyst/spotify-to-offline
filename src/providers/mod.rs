pub mod soulseek;
pub mod ytdlp;

use anyhow::Result;
use async_trait::async_trait;

use crate::import::TrackRow;

// ── Result type ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TrackOutcome {
    /// Downloaded successfully
    Ok { path: String, format: String },
    /// Already on disk (skip-existing / skip-music-dir hit). Path when known.
    Skipped { path: Option<String>, format: Option<String> },
    /// Track not found on this provider (try next)
    NotFound,
    /// Provider error (don't retry other providers for this)
    Failed { reason: String },
}

// ── Provider events ───────────────────────────────────────────────────────────

/// Streamed from providers to the orchestrator while a playlist runs.
#[derive(Debug, Clone)]
pub enum ProviderEvent {
    /// Human-readable log line for the TUI panel.
    Log(String),
    /// Emitted once at run start: how many tracks exist / are pending.
    TrackList { total: usize, existing: usize },
    /// A track reached a terminal state.
    TrackFinished { outcome: TerminalKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    Succeeded,
    Skipped,
    Failed,
}

// ── Provider trait ────────────────────────────────────────────────────────────

/// Implemented by each download provider.
/// Providers are constructed fresh per-playlist run.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Download a single track. Return outcome.
    #[allow(dead_code)]
    async fn download_track(
        &self,
        track:      &TrackRow,
        output_dir: &std::path::Path,
    ) -> Result<TrackOutcome>;

    /// Download an entire playlist (CSV) at once. `cancel` flips to true when
    /// the user aborts — implementations should kill their subprocess and bail.
    async fn download_playlist(
        &self,
        csv_path:   &std::path::Path,
        output_dir: &std::path::Path,
        ev_tx:      &tokio::sync::mpsc::UnboundedSender<ProviderEvent>,
        cancel:     tokio::sync::watch::Receiver<bool>,
    ) -> Result<Vec<(TrackRow, TrackOutcome)>>;
}

// ── Factory ───────────────────────────────────────────────────────────────────

use crate::config::Config;

pub fn build_providers(cfg: &Config) -> Vec<Box<dyn Provider>> {
    cfg.provider.order.iter().filter_map(|name| {
        match name.as_str() {
            "soulseek" => Some(Box::new(soulseek::SoulseekProvider::new(cfg)) as Box<dyn Provider>),
            "ytdlp"    => Some(Box::new(ytdlp::YtdlpProvider::new(cfg))    as Box<dyn Provider>),
            other      => { eprintln!("Unknown provider: {}", other); None }
        }
    }).collect()
}
