/// Import: extract an Exportify ZIP and convert CSVs to s2o's working format.
///
/// Exportify columns used:
///   Track Name → title
///   Album Name → album
///   Artist Name(s) → first artist only
///   Duration (ms) → length in seconds
///
/// Output: data_dir()/playlists_work/<playlist name>.csv
///         columns: artist, title, album, length

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::{raw_dir, work_dir};

// ── Row types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ExportifyRow {
    #[serde(rename = "Track Name", default)]
    track_name: String,
    #[serde(rename = "Album Name", default)]
    album_name: String,
    #[serde(rename = "Artist Name(s)", default)]
    artists: String,
    #[serde(rename = "Duration (ms)", default)]
    duration_ms: String,
}

/// Normalised per-track row. Written to work/ CSVs, read back during download + M3U.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRow {
    pub artist: String,
    pub title:  String,
    pub album:  String,
    pub length: u32,    // seconds
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run(zip_path: Option<&str>) -> Result<()> {
    let zip_path = match zip_path {
        Some(p) => PathBuf::from(p),
        None    => prompt_path()?,
    };
    if !zip_path.exists() {
        bail!("File not found: {}", zip_path.display());
    }

    println!("Extracting {}…", zip_path.display());
    extract_zip(&zip_path)?;

    println!("Converting CSVs…");
    let n = convert_all()?;
    println!("✓ {} playlist(s) ready in {}", n, work_dir().display());
    Ok(())
}

fn prompt_path() -> Result<PathBuf> {
    print!("Path to Exportify ZIP: ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(PathBuf::from(buf.trim()))
}

// ── Extract ───────────────────────────────────────────────────────────────────

fn extract_zip(zip_path: &Path) -> Result<()> {
    let raw = raw_dir();
    std::fs::create_dir_all(&raw)?;

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).context("Not a valid ZIP")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !name.ends_with(".csv") { continue; }
        let dest = raw.join(Path::new(&name).file_name().unwrap_or_default());
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        println!("  ← {}", dest.file_name().unwrap_or_default().to_string_lossy());
    }
    Ok(())
}

// ── Convert ───────────────────────────────────────────────────────────────────

pub fn convert_all() -> Result<usize> {
    let raw  = raw_dir();
    let work = work_dir();
    std::fs::create_dir_all(&work)?;

    let mut count = 0;
    for entry in std::fs::read_dir(&raw)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("csv") { continue; }
        let dest = work.join(path.file_name().unwrap());
        convert_csv(&path, &dest)?;
        count += 1;
    }
    Ok(count)
}

fn convert_csv(src: &Path, dest: &Path) -> Result<()> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_path(src)
        .with_context(|| format!("Cannot read {}", src.display()))?;

    // has_headers(true) on the writer causes the first struct serialize call
    // to automatically write a header row from field names
    let mut wtr = WriterBuilder::new()
        .has_headers(true)
        .from_path(dest)
        .with_context(|| format!("Cannot write {}", dest.display()))?;

    for result in rdr.deserialize::<ExportifyRow>() {
        let row = match result {
            Ok(r)  => r,
            Err(e) => { eprintln!("  skip row: {}", e); continue; }
        };

        let artist = row.artists
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        let length_ms: u64 = row.duration_ms.trim().parse().unwrap_or(0);

        wtr.serialize(TrackRow {
            artist,
            title:  row.track_name.trim().to_string(),
            album:  row.album_name.trim().to_string(),
            length: (length_ms / 1000) as u32,
        })?;
    }
    wtr.flush()?;
    Ok(())
}

// ── TUI-friendly import helpers ───────────────────────────────────────────────

/// Import from a path the user typed.
/// Accepts: a .zip file (extracts then converts) or a directory of CSVs (copies then converts).
pub fn import_path(path: &Path, log: impl Fn(String)) -> Result<usize> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    if path.is_file() && ext == "zip" {
        log(format!("Extracting {}…", path.display()));
        extract_zip(path)?;
        convert_all_with_log(log)
    } else if path.is_dir() {
        log(format!("Reading CSVs from {}…", path.display()));
        let raw = raw_dir();
        std::fs::create_dir_all(&raw)?;
        let mut copied = 0usize;
        for entry in std::fs::read_dir(path)?.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("csv") {
                let dest = raw.join(p.file_name().unwrap());
                std::fs::copy(&p, &dest)?;
                log(format!("  ← {}", p.file_name().unwrap_or_default().to_string_lossy()));
                copied += 1;
            }
        }
        if copied == 0 {
            anyhow::bail!("No CSV files found in {}", path.display());
        }
        convert_all_with_log(log)
    } else {
        anyhow::bail!("'{}' is not a ZIP file or directory", path.display());
    }
}

/// Try to find and import Exportify CSVs without the user specifying a path.
/// Search order:
///   1. work_dir() already has converted CSVs → nothing to do, return count
///   2. raw_dir()  has raw CSVs  → convert them
///   3. playlists_raw/ next to the s2o binary
///   4. playlists_raw/ in the current working directory
pub fn auto_detect(log: impl Fn(String)) -> Result<usize> {
    // If work dir already has content, just report it
    let work = work_dir();
    if work.exists() {
        let existing = csv_count(&work);
        if existing > 0 {
            log(format!("· {} playlist(s) already imported", existing));
            return Ok(existing);
        }
    }

    // Candidate raw folders, in preference order
    let exe_raw = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("playlists_raw")));
    let cwd_raw = std::env::current_dir()
        .ok()
        .map(|d| d.join("playlists_raw"));

    let candidates: Vec<PathBuf> = [
        Some(raw_dir()),
        exe_raw,
        cwd_raw,
    ]
    .into_iter()
    .flatten()
    .collect();

    for dir in candidates {
        if dir.exists() && csv_count(&dir) > 0 {
            log(format!("· Found CSVs in {}", dir.display()));
            // If this isn't raw_dir already, copy into raw_dir first
            let raw = raw_dir();
            if dir != raw {
                std::fs::create_dir_all(&raw)?;
                for entry in std::fs::read_dir(&dir)?.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("csv") {
                        std::fs::copy(&p, raw.join(p.file_name().unwrap()))?;
                    }
                }
            }
            return convert_all_with_log(log);
        }
    }

    Ok(0) // nothing found
}

fn csv_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|it| it.flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("csv"))
            .count())
        .unwrap_or(0)
}

fn convert_all_with_log(log: impl Fn(String)) -> Result<usize> {
    let n = convert_all()?;
    log(format!("✓ {} playlist(s) ready", n));
    Ok(n)
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Remove a playlist from s2o: its work + raw CSVs, generated M3Us, and the
/// sockseek index folder. Downloaded audio files are NOT touched — they live
/// in shared Artist/Album folders and may belong to other playlists.
/// Returns the paths that were actually deleted.
pub fn remove_playlist(cfg: &crate::config::Config, name: &str) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    let mut zap = |p: PathBuf| {
        if p.is_file() && std::fs::remove_file(&p).is_ok() {
            removed.push(p.display().to_string());
        }
    };

    zap(work_dir().join(format!("{}.csv", name)));
    zap(raw_dir().join(format!("{}.csv", name)));
    zap(cfg.paths.playlists_dir.join(format!("{}.m3u8", name)));
    zap(cfg.paths.music_root.join(format!("{}.m3u8", name)));   // legacy sldl output

    // sockseek's per-playlist index folder — remove the index, then the folder
    // itself only if that leaves it empty (paranoia against odd layouts).
    let index_dir = cfg.paths.music_root.join(name);
    zap(index_dir.join("_index.csv"));
    if index_dir.is_dir()
        && std::fs::read_dir(&index_dir).map(|mut d| d.next().is_none()).unwrap_or(false)
    {
        if std::fs::remove_dir(&index_dir).is_ok() {
            removed.push(index_dir.display().to_string());
        }
    }

    if removed.is_empty() {
        bail!("nothing found to remove for '{}'", name);
    }
    Ok(removed)
}

/// List all work CSVs sorted by name
pub fn list_playlists() -> Result<Vec<PathBuf>> {
    let work = work_dir();
    if !work.exists() { return Ok(vec![]); }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&work)? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("csv") {
            paths.push(p);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Parse a work CSV into TrackRows
pub fn load_playlist(path: &Path) -> Result<Vec<TrackRow>> {
    let mut rdr = ReaderBuilder::new().has_headers(true).from_path(path)?;
    rdr.deserialize().collect::<std::result::Result<_, _>>()
        .with_context(|| format!("Cannot parse {}", path.display()))
}
