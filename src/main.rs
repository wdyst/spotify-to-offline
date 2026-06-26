//! spotify-to-offline — Rust rewrite (work in progress)
//!
//! This is a scaffold for the Rust port of `run.py`.
//! See RUST_REWRITE.md for the porting plan and status.
//!
//! Until the rewrite is complete, use:
//!   Windows : run.bat   or   python run.py
//!   Linux   : ./run.sh  or   python3 run.py

mod config;
mod providers;
mod ui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "s2o", about = "Spotify → FLAC → DAP", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive menu (default)
    Menu,
    /// Download playlists via configured provider
    Download {
        /// Override provider (soulseek, ytdlp, custom)
        #[arg(short, long)]
        provider: Option<String>,
    },
    /// Generate M3U playlist files from local library
    M3u,
    /// Show current configuration
    Config,
}

fn main() {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Menu) {
        Commands::Menu          => ui::run_menu(),
        Commands::Download { .. } => todo!("Download not yet implemented — use run.py"),
        Commands::M3u           => todo!("M3U generation not yet implemented — use 4_generate_m3u.py"),
        Commands::Config        => config::show(),
    }
}
