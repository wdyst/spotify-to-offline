//! Download provider trait + implementations
//! TODO: port from Python providers in run.py

pub trait Provider {
    fn name(&self) -> &str;
    fn download(&self, csv_path: &str, output_dir: &str, m3u_path: &str) -> anyhow::Result<()>;
}

// Providers to implement:
//   SoulseekProvider  — wraps sockseek binary
//   YtdlpProvider     — wraps yt-dlp binary
//   CustomProvider    — user-defined shell command template
