/// SQLite state database.
///
/// Tracks every download outcome so you can:
///  - See what was downloaded and when
///  - Get a quality-warning report after a run
///  - (future) skip already-downloaded tracks

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;

use crate::config::data_dir;

pub fn db_path() -> PathBuf {
    data_dir().join("s2o.db")
}

pub fn open() -> Result<Connection> {
    let path = db_path();
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    let conn = Connection::open(&path)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS downloads (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            playlist    TEXT    NOT NULL,
            artist      TEXT    NOT NULL,
            title       TEXT    NOT NULL,
            album       TEXT    NOT NULL,
            provider    TEXT,
            status      TEXT    NOT NULL,
            file_path   TEXT,
            format      TEXT,
            created_at  TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_pl  ON downloads(playlist);
        CREATE INDEX IF NOT EXISTS idx_st  ON downloads(status);
        CREATE INDEX IF NOT EXISTS idx_ts  ON downloads(created_at);
        ",
    )?;
    Ok(())
}

// ── Status ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Status {
    Ok,
    NotFound,
    Failed,
    QualityWarn { wanted: String, got: String },
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok                     => write!(f, "ok"),
            Self::NotFound               => write!(f, "not_found"),
            Self::Failed                 => write!(f, "failed"),
            Self::QualityWarn { wanted, got } => write!(f, "quality_warn:{}>{}", wanted, got),
        }
    }
}

// ── Record ────────────────────────────────────────────────────────────────────

pub struct Record<'a> {
    pub playlist:  &'a str,
    pub artist:    &'a str,
    pub title:     &'a str,
    pub album:     &'a str,
    pub provider:  Option<&'a str>,
    pub status:    Status,
    pub file_path: Option<&'a str>,
    pub format:    Option<&'a str>,
}

pub fn insert(conn: &Connection, r: &Record<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads
             (playlist, artist, title, album, provider, status, file_path, format, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            r.playlist, r.artist, r.title, r.album,
            r.provider, r.status.to_string(),
            r.file_path, r.format,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

// ── Stats ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub total:         usize,
    pub ok:            usize,
    pub not_found:     usize,
    pub failed:        usize,
    pub quality_warns: usize,
}

pub fn stats_since(conn: &Connection, since: &DateTime<Utc>) -> Result<Stats> {
    let mut stmt = conn.prepare(
        "SELECT status FROM downloads WHERE created_at >= ?1"
    )?;
    let rows: Vec<String> = stmt
        .query_map(params![since.to_rfc3339()], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let mut s = Stats::default();
    s.total = rows.len();
    for row in &rows {
        match row.as_str() {
            "ok"        => s.ok        += 1,
            "not_found" => s.not_found += 1,
            "failed"    => s.failed    += 1,
            r if r.starts_with("quality_warn") => s.quality_warns += 1,
            _ => {}
        }
    }
    Ok(s)
}

/// Returns lines for the quality-warning section of the completion report.
#[allow(dead_code)]
pub fn quality_warns_since(conn: &Connection, since: &DateTime<Utc>) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT playlist, artist, title, status
           FROM downloads
          WHERE status LIKE 'quality_warn%'
            AND created_at >= ?1
          ORDER BY playlist, artist, title"
    )?;
    let rows = stmt.query_map(params![since.to_rfc3339()], |row| {
        Ok(format!(
            "[{}] {} - {} ({})",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?
    .filter_map(|r| r.ok())
    .collect();
    Ok(rows)
}
