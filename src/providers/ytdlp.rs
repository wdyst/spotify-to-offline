/// yt-dlp provider — per-track YouTube search + download.
///
/// Searches YouTube for "artist title" and downloads best audio in the
/// preferred format. No login required.
///
/// yt-dlp flags:
///   --default-search ytsearch1  — search YouTube, take first result
///   -x --audio-format flac      — extract audio in requested format
///   --audio-quality 0           — best quality
///   -o "%(artist)s/%(album)s/%(title)s.%(ext)s"
///   --no-playlist               — never download a playlist by accident
///   --quiet --no-warnings       — clean output for TUI log

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use super::{Provider, TrackOutcome};
use crate::config::Config;
use crate::import::{load_playlist, TrackRow};

pub struct YtdlpProvider {
    exe:    String,
    format: String,
}

impl YtdlpProvider {
    pub fn new(cfg: &Config) -> Self {
        Self {
            exe:    cfg.paths.ytdlp_path.clone(),
            format: cfg.download.preferred_format.clone(),
        }
    }

    async fn run_track(
        &self,
        track:      &TrackRow,
        output_dir: &Path,
        log_tx:     &UnboundedSender<String>,
    ) -> Result<TrackOutcome> {
        let query   = format!("{} {}", track.artist, track.title);
        let out_tpl = output_dir
            .join("%(uploader)s/%(album)s/%(title)s.%(ext)s")
            .to_string_lossy()
            .into_owned();

        let audio_fmt = match self.format.as_str() {
            "mp3"  => "mp3",
            "any"  => "best",
            _      => "flac",     // default to flac
        };

        let _ = log_tx.send(format!("ytdlp ↓ {} - {}", track.artist, track.title));

        let mut cmd = Command::new(&self.exe);
        cmd.args([
            "--default-search", "ytsearch1",
            "-x",
            "--audio-format",   audio_fmt,
            "--audio-quality",  "0",
            "-o",               &out_tpl,
            "--no-playlist",
            "--quiet",
            "--no-warnings",
            "--print",          "after_move:filepath",  // print actual output path
        ]);
        cmd.arg(&query);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let mut output_path = String::new();

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // The only stdout line should be the filepath
                if !line.trim().is_empty() {
                    output_path = line.trim().to_string();
                }
            }
        }
        if let Some(stderr) = child.stderr.take() {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = log_tx.send(format!("[ytdlp err] {}", line));
            }
        }

        let status = child.wait().await?;

        if !status.success() {
            return Ok(TrackOutcome::Failed {
                reason: format!("yt-dlp exited {}", status.code().unwrap_or(-1)),
            });
        }

        if output_path.is_empty() || !std::path::Path::new(&output_path).exists() {
            return Ok(TrackOutcome::NotFound);
        }

        let ext = std::path::Path::new(&output_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("?")
            .to_string();

        let _ = log_tx.send(format!("  ✓ {} [{}]", track.title, ext));

        Ok(TrackOutcome::Ok { path: output_path, format: ext })
    }
}

#[async_trait]
impl Provider for YtdlpProvider {
    fn name(&self) -> &'static str { "ytdlp" }

    async fn download_track(
        &self,
        track:      &TrackRow,
        output_dir: &Path,
    ) -> Result<TrackOutcome> {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        self.run_track(track, output_dir, &tx).await
    }

    async fn download_playlist(
        &self,
        csv_path:   &Path,
        output_dir: &Path,
        log_tx:     &UnboundedSender<String>,
    ) -> Result<Vec<(TrackRow, TrackOutcome)>> {
        let tracks = load_playlist(csv_path)?;
        std::fs::create_dir_all(output_dir)?;

        let mut results = Vec::with_capacity(tracks.len());
        for track in tracks {
            let outcome = self.run_track(&track, output_dir, log_tx).await
                .unwrap_or_else(|e| TrackOutcome::Failed { reason: e.to_string() });
            results.push((track, outcome));
        }
        Ok(results)
    }
}
