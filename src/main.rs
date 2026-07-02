mod config;
mod db;
mod dap;
mod import;
mod install;
mod download;
mod m3u;
mod tags;
mod notify;
mod providers;
mod sldl_setup;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name    = "s2o",
    about   = "spotify-to-offline — Spotify → FLAC → DAP",
    version = "3.1.0",
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
    /// Per-playlist download status + library totals
    Status,
    /// Remove a playlist (CSVs, M3U, history — audio files are kept)
    Remove {
        /// Playlist name (CSV filename without extension)
        playlist: String,
    },
    /// Show current configuration
    Config,
    /// Install s2o to a permanent location and add to PATH
    Install,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Install doesn't need a config — run it immediately and exit
    if matches!(cli.command, Some(Cmd::Install)) {
        return install::run();
    }

    // First-run: no config yet → wizard (skip for non-interactive commands)
    let needs_config = !config::config_path().exists();
    let skip_wizard  = matches!(cli.command, Some(Cmd::Setup) | Some(Cmd::Config));
    if needs_config && !skip_wizard {
        println!("Welcome to spotify-to-offline! Running first-time setup…\n");
        config::run_setup()?;
        println!();
    }

    match cli.command.unwrap_or(Cmd::Ui) {
        Cmd::Ui                    => ui::run().await?,
        Cmd::Setup                 => config::run_setup()?,
        Cmd::Import { zip }        => import::run(zip.as_deref()).await?,
        Cmd::Download { playlist } => {
            let cfg = config::load()?;
            download::run_all_cli(&cfg, playlist.as_deref()).await?;
        }
        Cmd::M3u { profile } => {
            let cfg = config::load()?;
            m3u::run(&cfg, profile.as_deref())?;
        }
        Cmd::Status => {
            let cfg = config::load()?;
            let playlists = import::list_playlists()?;
            if playlists.is_empty() {
                println!("No playlists imported — run `s2o import` first.");
                return Ok(());
            }
            let (mut tracks_total, mut tracks_done) = (0usize, 0usize);
            println!("{:>9}  playlist", "on disk");
            for csv in &playlists {
                let name   = csv.file_stem().unwrap_or_default().to_string_lossy();
                let tracks = import::load_playlist(csv).unwrap_or_default();
                let index  = providers::soulseek::parse_index(
                    &cfg.paths.music_root.join(name.as_ref()).join("_index.csv"),
                );
                let done = tracks.iter()
                    .filter(|t| index.get(&t.title.to_lowercase()).map(|p| p.exists()).unwrap_or(false))
                    .count();
                tracks_total += tracks.len();
                tracks_done  += done;
                let mark = if !tracks.is_empty() && done >= tracks.len() { "✓" }
                           else if done > 0 { "·" } else { " " };
                println!("{:>4}/{:<4} {} {}", done, tracks.len(), mark, name);
            }
            println!("\n{} of {} tracks on disk across {} playlists",
                tracks_done, tracks_total, playlists.len());
        }
        Cmd::Remove { playlist } => {
            let cfg = config::load()?;
            for p in import::remove_playlist(&cfg, &playlist)? {
                println!("removed {}", p);
            }
            let n = db::open().and_then(|c| db::delete_playlist(&c, &playlist))?;
            if n > 0 { println!("cleared {} history rows", n); }
            println!("✓ '{}' removed (audio files kept)", playlist);
        }
        Cmd::Config  => config::show()?,
        Cmd::Install => unreachable!(), // handled above
    }

    Ok(())
}
