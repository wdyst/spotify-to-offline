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
    /// Track not found on this provider (try next)
    NotFound,
    /// Provider error (don't retry other providers for this)
    Failed { reason: String },
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

    /// Download an entire playlist (CSV) at once.
    /// Default: serial loop over download_track. Soulseek overrides this
    /// to run the whole CSV through sockseek in one subprocess call.
    async fn download_playlist(
        &self,
        csv_path:   &std::path::Path,
        output_dir: &std::path::Path,
        log_tx:     &tokio::sync::mpsc::UnboundedSender<String>,
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
