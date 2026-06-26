/// Self-installation: copies the s2o binary to a permanent location and adds
/// it to the user's PATH — no package manager or admin rights required.
///
/// Install locations:
///   Windows  →  %LOCALAPPDATA%\s2o\bin\s2o.exe   (user PATH via registry)
///   Linux    →  ~/.local/bin/s2o                  (added to shell profiles)
///   macOS    →  ~/.local/bin/s2o                  (added to shell profiles)
///
/// Idempotent: running `s2o install` again is always safe.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn run() -> Result<()> {
    let current = std::env::current_exe()
        .context("Cannot determine path to current executable")?;

    let (bin_dir, exe_name) = install_location()?;
    let target = bin_dir.join(&exe_name);

    println!();

    // Create the install directory if needed
    std::fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Cannot create install directory: {}", bin_dir.display()))?;

    // Only copy if we're not already running from the install location
    let already_installed = std::fs::canonicalize(&current).ok()
        == std::fs::canonicalize(&target).ok();

    if already_installed {
        println!("· Running from install location — skipping copy.");
    } else {
        // On Windows the old binary may be locked; remove first, then copy
        #[cfg(windows)]
        let _ = std::fs::remove_file(&target);

        std::fs::copy(&current, &target)
            .with_context(|| format!("Cannot copy binary to {}", target.display()))?;

        println!("✓ Installed  →  {}", target.display());
    }

    ensure_on_path(&bin_dir)?;

    println!();
    println!("  Open a new terminal and you're set:");
    println!("    s2o         — launch the TUI (runs setup wizard on first use)");
    println!("    s2o setup   — configure paths, credentials, and provider order");
    println!();

    Ok(())
}

// ── Install location ──────────────────────────────────────────────────────────

#[cfg(windows)]
fn install_location() -> Result<(PathBuf, String)> {
    // %LOCALAPPDATA%\s2o\bin\  — no admin rights, per-user
    let dir = dirs::data_local_dir()
        .context("Cannot find %LOCALAPPDATA%")?
        .join("s2o")
        .join("bin");
    Ok((dir, "s2o.exe".into()))
}

#[cfg(not(windows))]
fn install_location() -> Result<(PathBuf, String)> {
    // ~/.local/bin/ — the standard XDG user binary location,
    // already on PATH on most modern Linux distros and macOS.
    let dir = dirs::home_dir()
        .context("Cannot find home directory")?
        .join(".local")
        .join("bin");
    Ok((dir, "s2o".into()))
}

// ── PATH management ───────────────────────────────────────────────────────────

#[cfg(windows)]
fn ensure_on_path(bin_dir: &Path) -> Result<()> {
    use std::process::Command;

    let dir_str = bin_dir.to_string_lossy().to_string();

    // Read the current user PATH from the Windows registry via PowerShell.
    // No extra crates needed — PowerShell ships with every modern Windows install.
    let out = Command::new("powershell")
        .args([
            "-NoProfile", "-NonInteractive", "-Command",
            "[Environment]::GetEnvironmentVariable('PATH','User')",
        ])
        .output()
        .context("Cannot run PowerShell to read user PATH")?;

    let current_path = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Case-insensitive check (Windows paths are case-insensitive)
    let already_present = current_path
        .split(';')
        .any(|segment| segment.trim().eq_ignore_ascii_case(&dir_str));

    if already_present {
        println!("· {} is already on your PATH.", dir_str);
        return Ok(());
    }

    // Build new PATH value and write it back
    let new_path = if current_path.trim_end_matches(';').is_empty() {
        dir_str.clone()
    } else {
        format!("{};{}", current_path.trim_end_matches(';'), dir_str)
    };

    // Escape single quotes for PowerShell string literals
    let escaped = new_path.replace('\'', "''");
    let set_cmd = format!(
        "[Environment]::SetEnvironmentVariable('PATH','{}','User')",
        escaped
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &set_cmd])
        .status()
        .context("Cannot run PowerShell to update user PATH")?;

    if !status.success() {
        anyhow::bail!(
            "PATH update failed (PowerShell exit {:?}). \
             You can add it manually:\n  \
             [Environment]::SetEnvironmentVariable('PATH', $env:PATH + ';{}', 'User')",
            status.code(),
            dir_str
        );
    }

    println!("✓ Added to user PATH  →  {}", dir_str);
    Ok(())
}

#[cfg(not(windows))]
fn ensure_on_path(bin_dir: &Path) -> Result<()> {
    use std::io::Write;

    let dir_str = bin_dir.to_string_lossy().to_string();

    // Check if already on PATH in the current session
    let on_path_now = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|p| p == dir_str);

    if on_path_now {
        println!("· {} is already on your PATH.", dir_str);
        return Ok(());
    }

    // Append the export line to every shell profile file that already exists.
    // Only add to profiles that don't already reference this directory.
    let home = dirs::home_dir().context("Cannot find home directory")?;
    let export_line = format!(
        "\n# Added by s2o install\nexport PATH=\"$PATH:{}\"\n",
        dir_str
    );

    let profiles = [".bashrc", ".bash_profile", ".zshrc", ".profile"];
    let mut patched: Vec<String> = Vec::new();

    for name in &profiles {
        let profile_path = home.join(name);
        if profile_path.exists() {
            let content = std::fs::read_to_string(&profile_path).unwrap_or_default();
            if !content.contains(&dir_str) {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&profile_path)
                    .and_then(|mut f| f.write_all(export_line.as_bytes()))
                    .with_context(|| format!("Cannot write to ~/{}", name))?;
                patched.push(format!("~/{}", name));
            }
        }
    }

    if patched.is_empty() {
        // ~/.local/bin is usually pre-configured; no profile found to patch
        println!("· No shell profile found to patch.");
        println!("  Add this line to your shell profile manually:");
        println!("    export PATH=\"$PATH:{}\"", dir_str);
    } else {
        println!("✓ Added to PATH in: {}", patched.join(", "));
    }

    Ok(())
}
