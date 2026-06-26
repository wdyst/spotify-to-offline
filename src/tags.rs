/// Audio tag normalization using lofty.
///
/// Writes clean Artist / Title / Album tags from Exportify CSV metadata
/// after download, so the library has consistent tags regardless of what
/// sockseek or yt-dlp embedded originally.

use anyhow::{bail, Result};
use lofty::config::WriteOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;

use crate::import::TrackRow;

pub fn write(file_path: &str, track: &TrackRow) -> Result<()> {
    let path = Path::new(file_path);
    if !path.exists() { return Ok(()); }

    let mut tagged_file = Probe::open(path)
        .map_err(|e| anyhow::anyhow!("lofty open {}: {}", path.display(), e))?
        .guess_file_type()
        .map_err(|e| anyhow::anyhow!("lofty guess {}: {}", path.display(), e))?
        .read()
        .map_err(|e| anyhow::anyhow!("lofty read {}: {}", path.display(), e))?;

    // Insert a tag if none exists
    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        tagged_file.insert_tag(lofty::tag::Tag::new(tag_type));
    }

    if let Some(tag) = tagged_file.primary_tag_mut() {
        tag.set_artist(track.artist.clone());
        tag.set_title(track.title.clone());
        tag.set_album(track.album.clone());
    } else {
        bail!("could not create tag for {}", path.display());
    }

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| anyhow::anyhow!("lofty write {}: {}", path.display(), e))?;

    Ok(())
}
