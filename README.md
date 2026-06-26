# spotify-to-offline

Move your Spotify playlists to a local FLAC library, then generate M3U playlist files
ready for any portable music player. Tested on the Snowsky Echo Mini; works on FiiO,
Hiby, Shanling, and anything else that reads M3U.

**What it does:**
1. Exports your Spotify playlists via [Exportify](https://exportify.net)
2. Batch-downloads every track from your configured provider (Soulseek, yt-dlp, or custom)
3. Generates M3U playlist files with relative paths that work on any DAP or SD card

---

## Requirements

- **Python 3.8+** — no third-party packages needed (stdlib only)
- **A download provider** — pick one:
  - **Soulseek** (default): free account at [slsknet.org](https://www.slsknet.org) + [Sockseek](https://github.com/fiso64/slsk-batchdl) (installed automatically)
  - **yt-dlp**: `pip install yt-dlp` or [github.com/yt-dlp/yt-dlp](https://github.com/yt-dlp/yt-dlp)
  - **Custom**: any command you want — set a template in Settings
- **A VPN** *(Soulseek only)*: Soulseek is P2P and exposes your IP to peers.
  [Mullvad](https://mullvad.net) and [ProtonVPN](https://protonvpn.com) are solid choices.

---

## Quick Start

```
1. Clone or download this repo
2. Double-click run.bat  (Windows)  OR  ./run.sh  (Linux / macOS)
3. Follow the menu
```

That's it. The launcher walks you through every step.

---

## The Menu

```
  +------------------------------------------------+
  |           spotify-to-offline  v2.0             |
  |       Spotify -> FLAC -> Snowsky / DAP         |
  +------------------------------------------------+

  [1]  Set Soulseek credentials
  [2]  Install Sockseek
  [3]  Import playlists  (opens Exportify in browser)
  [4]  Download  (via configured provider)
  [5]  Generate M3U files for Snowsky / DAP
  [6]  Full run  (steps 3 → 4 → 5)
  ──────────────────────────────────────────────────
  [s]  Settings  (paths, provider, custom commands)
  [q]  Quit
```


## Step-by-step

### [1] Set Soulseek credentials

Enter your username and password — saved to `config.ini` (gitignored, never committed).
Only needed once. Skip this if you're using yt-dlp or a custom provider.

### [2] Install Sockseek

Downloads the correct [Sockseek](https://github.com/fiso64/slsk-batchdl) binary for your OS
(Windows, Linux, or macOS) and drops it in the script folder. Skip if using yt-dlp.

### [3] Import playlists

Opens [exportify.net](https://exportify.net) in your browser. Sign in with Spotify and click
**Export All** to get a ZIP of CSVs. Drag the ZIP into the terminal when prompted — the
launcher extracts and converts everything automatically.

### [4] Download

> **⚠ Connect to a VPN before this step if using Soulseek.** It's P2P and your real IP is
> visible to every peer you download from.

Runs your configured provider against each playlist in sequence.

- **Soulseek**: prefers FLAC, falls back to MP3/M4A if lossless isn't on the network
- **yt-dlp**: searches YouTube Music per track and downloads as FLAC
- **Custom**: runs your command template for each track

Downloads land in your configured music root (`~/Music` by default), organised as
`Artist/Album/Title.flac`. Already-downloaded tracks are skipped — **Ctrl+C** pauses safely
and you can resume any time by running option 4 again.

Large libraries (1000+ tracks) will take several hours. Leave it overnight.

### [5] Generate M3U

Scans your music library, fuzzy-matches each playlist track to a local file, and writes one
`.m3u` per playlist to your playlists directory (`~/Music/Playlists` by default).

Paths are **relative** (e.g. `../Artist/Album/title.flac`) so they work on your PC and on
a DAP's SD card without any editing. Unmatched tracks go to `m3u_unmatched.txt`.

Re-run any time you add music — it always reflects what's actually on disk.

---

## Settings  `[s]`

| Setting | Default | Description |
|---|---|---|
| Music root | `~/Music` | Where downloads and existing files live |
| Playlists dir | `~/Music/Playlists` | Where M3U files are written |
| Provider | `soulseek` | `soulseek`, `ytdlp`, or `custom` |
| Sockseek path | *(auto)* | Path to sockseek binary if not in script folder |
| yt-dlp path | `yt-dlp` | Path or command name for yt-dlp |
| Custom command | — | Command template with `{artist}` `{title}` `{album}` `{output}` |

All settings are saved to `config.ini` (gitignored).

### Example: custom provider

Set a custom command like:

```
yt-dlp "ytsearch1:{artist} {title}" -x --audio-format flac -o {output}/{artist}/{title}.%(ext)s
```

Or use any other tool that can take artist/title/album/output arguments.

---

## Output Structure

```
Music/
├── Artist Name/
│   └── Album Name/
│       └── Track Title.flac      ← downloaded
├── blink-182/                     ← existing library (any structure works)
│   └── ...
└── Playlists/
    ├── Liked_Songs.m3u
    ├── pop_punk.m3u
    └── ...                        ← one per playlist, DAP-ready
```

---


## Snowsky Echo Mini Setup

1. Copy your `Music/` folder contents to the **root of your SD card**
2. The `Playlists/` folder should be at the same level as your artist folders
3. Insert the card — navigate to Playlists in the player menu

The relative paths in the M3U files resolve correctly regardless of the card's drive letter
or mount point. Should also work on FiiO, Hiby, Shanling, and other DAPs with M3U support.

---

## Linux / macOS

All Python scripts are fully cross-platform. The shell entry point is `run.sh`:

```bash
chmod +x run.sh
./run.sh
```

Sockseek is available for Linux and macOS — option 2 (Install Sockseek) downloads the
correct binary automatically.

---

## Rust Rewrite (WIP)

A Rust port is in progress. See [RUST_REWRITE.md](RUST_REWRITE.md) for status.
The Python implementation remains the primary, fully functional version.

```bash
# Build (once Rust port is complete)
cargo build --release
./target/release/s2o
```

---

## Advanced / Manual Use

The individual scripts still work if you prefer to run steps directly:

| Script | What it does |
|---|---|
| `1_setup.py` | Cross-platform Sockseek installer (Windows / Linux / macOS) |
| `1_setup_sldl.ps1` | PowerShell Sockseek installer (Windows only, legacy) |
| `2_prep_csvs.py` | Converts Exportify CSVs to Sockseek format |
| `3_download_all.ps1` | Batch download via Sockseek (Windows, edit creds at top) |
| `4_generate_m3u.py` | Generates M3U files — respects `MUSIC_ROOT` and `PLAYLISTS_DIR` env vars |

---

## Notes

- **VPN:** Soulseek is P2P — your real IP is visible to every peer you download from.
  [Mullvad](https://mullvad.net) and [ProtonVPN](https://protonvpn.com) work well.
- **Rate limits:** Sockseek searches at a conservative rate by default. Pushing
  `--searches-per-time` too high risks temporary 30-minute bans.
- **Niche tracks:** Check `m3u_unmatched.txt` for anything Soulseek couldn't find — source
  those via Bandcamp, direct purchase, or switch to yt-dlp as a fallback provider.
- **yt-dlp fallback:** Sockseek also supports `--yt-dlp` natively to fall back to YouTube
  for tracks not found on Soulseek. Requires yt-dlp on your PATH.
- **Re-running:** All steps are safe to re-run. Downloads skip existing files; M3Us are
  fully regenerated each time.

---

## License

MIT — do whatever you want with it.
