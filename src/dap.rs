/// DAP (Digital Audio Player) M3U output profiles.
///
/// Research notes on real devices:
///
/// UNIVERSAL / FiiO / Hiby / Shanling / Astell&Kern / iBasso / Cayin / Snowsky:
///   - All read extended M3U8 (UTF-8, no BOM)
///   - Prefer relative paths with forward slashes relative to the M3U file location
///   - FiiO M11/M15/M17, Hiby R6III, Shanling M6 Ultra, AK SE300, iBasso DX300:
///     all confirmed relative-path M3U support
///   - Snowsky Echo Mini: confirmed relative paths work from SD card
///
/// Sony Walkman NW-A/WM1/ZX series:
///   - Uses Content Transfer (or drag-and-drop on newer models)
///   - Playlists use forward-slash paths relative to the SD card root (no "..")
///   - Older NW-A100 series needed UTF-8 BOM in M3U
///   - Newer NW-A306 / ZX707: no BOM needed
///
/// PC — foobar2000, VLC, Windows Media Player:
///   - Extended M3U, absolute paths, system-native separators
///   - VLC and foobar2000 handle both slash styles
///
/// Simple M3U:
///   - No #EXTINF lines — works on every player that reads M3U at all
///   - Last resort for very old/embedded firmware

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PathStyle {
    /// ../Artist/Album/track.flac  — relative from M3U file location
    Relative,
    /// /Artist/Album/track.flac   — absolute from music root / SD-card root
    SdRoot,
    /// /full/system/path/...       — full absolute OS path
    Absolute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Sep {
    Forward,  // /
    Backward, // \
    Native,   // OS default
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapProfile {
    pub name:        String,
    pub description: String,
    pub path_style:  PathStyle,
    pub sep:         Sep,
    /// Include UTF-8 BOM at file start (needed by old Sony firmware)
    pub utf8_bom:    bool,
    /// Include #EXTINF metadata lines
    pub extended:    bool,
    /// Override music root for this profile
    pub music_root:  Option<PathBuf>,
    /// Override M3U output dir for this profile
    pub m3u_dir:     Option<PathBuf>,
}

impl DapProfile {
    /// Format a track path for insertion in this profile's M3U.
    /// `track_path`: absolute path to audio file on disk
    /// `m3u_dir`:    directory the M3U file will live in
    /// `music_root`: global music root (used as SD-card root for SdRoot style)
    pub fn format_path(
        &self,
        track_path: &Path,
        m3u_dir:    &Path,
        music_root: &Path,
    ) -> String {
        let sep = match self.sep {
            Sep::Forward  => '/',
            Sep::Backward => '\\',
            Sep::Native   => std::path::MAIN_SEPARATOR,
        };

        let raw = match self.path_style {
            PathStyle::Relative => {
                relative_path(m3u_dir, track_path)
                    .to_string_lossy()
                    .into_owned()
            }
            PathStyle::SdRoot => {
                let suffix = track_path
                    .strip_prefix(music_root)
                    .unwrap_or(track_path)
                    .to_string_lossy()
                    .into_owned();
                format!("/{}", suffix)
            }
            PathStyle::Absolute => track_path.to_string_lossy().into_owned(),
        };

        // Normalize separator
        if sep == '/' {
            raw.replace('\\', "/")
        } else if sep == '\\' {
            raw.replace('/', "\\")
        } else {
            raw
        }
    }
}

/// Compute a relative path from `from` directory to `to` file.
fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_c: Vec<_> = from.components().collect();
    let to_c:   Vec<_> = to.components().collect();

    let common = from_c.iter().zip(to_c.iter()).take_while(|(a, b)| a == b).count();
    let ups     = from_c.len() - common;

    let mut result = PathBuf::new();
    for _ in 0..ups { result.push(".."); }
    for c in &to_c[common..] { result.push(c); }
    result
}

/// Built-in defaults — returned from `default_profiles()`.
/// Users can extend or override these in config.toml.
pub fn default_profiles() -> Vec<DapProfile> {
    vec![
        DapProfile {
            name:        "universal".into(),
            description: "Universal — FiiO, Hiby, Shanling, Astell&Kern, iBasso, Cayin, Snowsky".into(),
            path_style:  PathStyle::Relative,
            sep:         Sep::Forward,
            utf8_bom:    false,
            extended:    true,
            music_root:  None,
            m3u_dir:     None,
        },
        DapProfile {
            name:        "sony".into(),
            description: "Sony Walkman NW-A / NW-WM1 / ZX series".into(),
            path_style:  PathStyle::SdRoot,
            sep:         Sep::Forward,
            utf8_bom:    false,   // set true for NW-A100 series if playlists show empty
            extended:    true,
            music_root:  None,
            m3u_dir:     None,
        },
        DapProfile {
            name:        "sony-bom".into(),
            description: "Sony Walkman (older NW-A100 / older firmware — needs UTF-8 BOM)".into(),
            path_style:  PathStyle::SdRoot,
            sep:         Sep::Forward,
            utf8_bom:    true,
            extended:    true,
            music_root:  None,
            m3u_dir:     None,
        },
        DapProfile {
            name:        "pc".into(),
            description: "PC — foobar2000, VLC, Windows Media Player".into(),
            path_style:  PathStyle::Absolute,
            sep:         Sep::Native,
            utf8_bom:    false,
            extended:    true,
            music_root:  None,
            m3u_dir:     None,
        },
        DapProfile {
            name:        "simple".into(),
            description: "Simple M3U — no #EXTINF, maximum firmware compatibility".into(),
            path_style:  PathStyle::Relative,
            sep:         Sep::Forward,
            utf8_bom:    false,
            extended:    false,
            music_root:  None,
            m3u_dir:     None,
        },
    ]
}
