mod config;
mod db;
mod dap;
mod import;
mod download;
mod m3u;
mod tags;
mod notify;
mod providers;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name    = "s2o",
    about   = "spotify-to-offline — Spotify → FLAC → DAP",
    version = "2.0.0",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive TUI (default)
    Ui,
    /// First-time setup wizard
    Setup,
    /// Import playlists from an Exportify ZIP
    Import {
        /// Path to the Exportify ZIP file (prompted if omitted)
        zip: Option<String>,
    },
    /// Download tracks via configured provider(s)
    Download {
        /// Only process this playlist (CSV filename without extension)
        #[arg(short, long)]
        playlist: Option<String>,
    },
    /// Generate M3U playlist files
    M3u {
        /// DAP profile to use (defaults to first in config)
        #[arg(short, long)]
        profile: Option<String>,
    },
    /// Show current configuration
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // First-run: no config yet → wizard
    if !config::config_path().exists() && !matches!(cli.command, Some(Cmd::Setup)) {
        println!("Welcome to spotify-to-offline! Running first-time setup…\n");
        config::run_setup()?;
        println!();
    }

    match cli.command.unwrap_or(Cmd::Ui) {
        Cmd::Ui                  => ui::run().await?,
        Cmd::Setup               => config::run_setup()?,
        Cmd::Import { zip }      => import::run(zip.as_deref()).await?,
        Cmd::Download { playlist } => {
            let cfg = config::load()?;
            download::run_all_cli(&cfg, playlist.as_deref()).await?;
        }
        Cmd::M3u { profile } => {
            let cfg = config::load()?;
            m3u::run(&cfg, profile.as_deref())?;
        }
        Cmd::Config => config::show()?,
    }

    Ok(())
}
