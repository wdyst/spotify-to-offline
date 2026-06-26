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

// ── Public helpers ────────────────────────────────────────────────────────────

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
