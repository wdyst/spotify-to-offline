///
/// Downloads the latest sldl (slsk-batchdl) binary from GitHub and places it
/// next to the s2o executable so the Soulseek provider can find it.
///
/// Uses the system `curl` executable (built into Windows 10+ and every
/// Linux/macOS install) so we don't need an HTTP client dependency.

use anyhow::{Context, Result};
use std::path::PathBuf;

const GITHUB_API: &str =
    "https://api.github.com/repos/fiso64/slsk-batchdl/releases/latest";

/// Synchronous download — call via `tokio::task::spawn_blocking` from the TUI.
pub fn download_sldl(log: impl Fn(String)) -> Result<PathBuf> {
    log("  Fetching latest sldl release from GitHub…".into());

    // ── 1. Fetch release metadata ─────────────────────────────────────────────
    let api_out = std::process::Command::new("curl")
        .args(["-s", "-L", "-H", "User-Agent: s2o/3.0", GITHUB_API])
        .output()
        .context("curl not found — install curl or download sldl manually from github.com/fiso64/slsk-batchdl")?;

    if !api_out.status.success() {
        anyhow::bail!("GitHub API request failed (status {})", api_out.status);
    }

    let release: serde_json::Value = serde_json::from_slice(&api_out.stdout)
        .context("failed to parse GitHub API response")?;

    let tag = release["tag_name"].as_str().unwrap_or("unknown");
    log(format!("  Latest sldl: {}", tag));

    // ── 2. Find the right asset for this platform ─────────────────────────────
    let kw = platform_keyword();
    let assets = release["assets"]
        .as_array()
        .context("no assets in release")?;

    let asset = assets
        .iter()
        .find(|a| {
            let name = a["name"].as_str().unwrap_or("");
            name.contains(kw) && name.ends_with(".zip")
        })
        .context(format!("no '{}' zip asset found in release {}", kw, tag))?;

    let url      = asset["browser_download_url"].as_str().context("missing download URL")?;
    let filename = asset["name"].as_str().unwrap_or("sldl.zip");

    log(format!("  Downloading {}…", filename));

    // ── 3. Download to temp ───────────────────────────────────────────────────
    let tmp = std::env::temp_dir().join(filename);
    let status = std::process::Command::new("curl")
        .args(["-L", "-o", &tmp.to_string_lossy(), url])
        .status()
        .context("curl failed to start download")?;

    if !status.success() {
        anyhow::bail!("download failed (curl exit {})", status);
    }

    // ── 4. Extract exe to the same directory as s2o ───────────────────────────
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(PathBuf::from)
        .context("cannot determine s2o exe directory")?;

    log("  Extracting…".into());
    let out = extract_exe(&tmp, &exe_dir)?;

    // Clean up temp zip
    let _ = std::fs::remove_file(&tmp);

    Ok(out)
}

/// Returns the platform keyword used to match the GitHub release asset name.
#[cfg(target_os = "windows")]
fn platform_keyword() -> &'static str { "win" }
#[cfg(target_os = "linux")]
fn platform_keyword() -> &'static str { "linux" }
#[cfg(target_os = "macos")]
fn platform_keyword() -> &'static str { "osx" }
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn platform_keyword() -> &'static str { "linux" }

/// Extracts the executable from the zip into `out_dir` as `sldl[.exe]`.
fn extract_exe(zip_path: &PathBuf, out_dir: &PathBuf) -> Result<PathBuf> {
    use std::io::Read;

    let file    = std::fs::File::open(zip_path)?;
    let mut arc = zip::ZipArchive::new(file)?;

    for i in 0..arc.len() {
        let mut entry = arc.by_index(i)?;
        let raw_name  = entry.name().to_string();

        // We're looking for the main executable — skip directories.
        if raw_name.ends_with('/') {
            continue;
        }
        let basename = raw_name.split('/').last().unwrap_or("").to_string();

        #[cfg(windows)]
        let is_target = basename.ends_with(".exe");
        #[cfg(not(windows))]
        let is_target = !basename.contains('.') && !basename.is_empty();

        if is_target {
            #[cfg(windows)]
            let out_name = "sldl.exe";
            #[cfg(not(windows))]
            let out_name = "sldl";

            let out_path = out_dir.join(out_name);

            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            std::fs::write(&out_path, &buf)?;

            // Make executable on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &out_path,
                    std::fs::Permissions::from_mode(0o755),
                )?;
            }

            return Ok(out_path);
        }
    }

    anyhow::bail!("no executable found inside the downloaded zip")
}

/// Quick check: is sldl already accessible?
pub fn sldl_found() -> bool {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from));

    which::which("sldl").is_ok()
        || which::which("sldl.exe").is_ok()
        || exe_dir
            .map(|d| d.join("sldl.exe").exists() || d.join("sldl").exists())
            .unwrap_or(false)
}
