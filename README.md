# spotify-to-offline

Move your Spotify playlists to a local FLAC library, then generate M3U playlist files
ready for a portable music player (tested on Snowsky Echo Mini).

**What it does:**
1. Exports your Spotify playlists via [Exportify](https://exportify.net)
2. Batch-downloads every track from Soulseek, preferring FLAC, skipping anything you already own
3. Generates M3U playlist files with relative paths so they work on any DAP or media player

---

## Requirements

- **Windows** (scripts use PowerShell; the Python files work cross-platform)
- **Python 3.8+** — no third-party packages needed (stdlib only)
- **A Soulseek account** — free at [slsknet.org](https://www.slsknet.org)
- **A VPN** — Soulseek is P2P and exposes your IP to peers.
  [Mullvad](https://mullvad.net) and [ProtonVPN](https://protonvpn.com) are solid P2P-friendly options.

---

## Quick Start

```
1. Clone or download this repo
2. Double-click run.bat
3. Follow the menu
```

That's it. The launcher walks you through every step.

---


## Using the Launcher

Double-click **`run.bat`** (or run `python run.py` in a terminal). You'll see a menu:

```
  +----------------------------------------------+
  |          spotify-to-offline  v1.0            |
  |      Spotify -> FLAC -> Snowsky / DAP        |
  +----------------------------------------------+

  [1]  Set Soulseek credentials
  [2]  Import playlists  (opens Exportify in browser)
  [3]  Download FLACs from Soulseek
  [4]  Generate M3U files for Snowsky / DAP
  [5]  Full run  (steps 1-4 in sequence)
  [q]  Quit
```

### Step 1 — Set Soulseek credentials

Enter your Soulseek username and password. These are saved locally to `config.ini`
(gitignored — never committed) so you only need to do this once.

Don't have an account? Sign up free at [slsknet.org](https://www.slsknet.org).

### Step 2 — Import playlists

The launcher opens [exportify.net](https://exportify.net) in your browser. Sign in with
Spotify and click **Export All** to download a ZIP of CSVs (one per playlist).

Drag the downloaded ZIP into the terminal window when prompted, or paste the path.
The launcher extracts and converts everything automatically.

### Step 3 — Download FLACs

> **⚠ Connect to a VPN before this step.** Soulseek is peer-to-peer — your real IP address
> is visible to every user you download from.

The launcher runs [Sockseek](https://github.com/fiso64/slsk-batchdl) against each playlist
in sequence. Downloads land in `C:\Users\<you>\Music\{Artist}\{Album}\{Title}.flac`.

- Prefers FLAC; falls back to MP3/M4A if lossless isn't available
- Skips tracks already in your music library
- **Ctrl+C** pauses — run option 3 again to resume exactly where you left off

Large libraries (1000+ tracks) will take several hours. Leave it running overnight.

### Step 4 — Generate M3U files

Scans your music library, fuzzy-matches each playlist track to a local file, and writes
one `.m3u` file per playlist to `C:\Users\<you>\Music\Playlists\`.

M3U paths are **relative** (e.g. `../Artist/Album/title.flac`) so the playlist files
work correctly on your PC and on a DAP's SD card without any editing.

Any tracks that couldn't be matched are logged to `m3u_unmatched.txt` for manual review.

Re-run option 4 any time you add more music — it always reflects your current library.

---


## Output Structure

```
Music\
├── Artist Name\
│   └── Album Name\
│       └── Track Title.flac        ← downloaded by Sockseek
├── blink-182\                        ← existing library (any structure works)
│   └── ...
└── Playlists\
    ├── Liked_Songs.m3u
    ├── pop_punk.m3u
    └── ...                          ← one file per playlist, Snowsky-ready
```

---

## Snowsky Echo Mini Setup

1. Copy everything inside your `Music\` folder to the **root of your SD card**
2. The `Playlists\` folder should sit at the same level as your artist folders
3. Insert the card and navigate to Playlists in the player menu

The M3U files use relative paths (`../Artist/Album/title.flac`) so they resolve
correctly regardless of the card's drive letter or mount point.

This should also work with FiiO, Hiby, Shanling, and other DAPs that support M3U playlists.

---

## Configuration

All paths are set at the top of each script. The launcher stores credentials in `config.ini`
(gitignored). Key path variables:

| Script | Variable | Default |
|---|---|---|
| `3_download_all.ps1` | `$MusicRoot` | `C:\Users\<you>\Music` |
| `4_generate_m3u.py` | `MUSIC_ROOT` | `C:\Users\<you>\Music` |
| `4_generate_m3u.py` | `PLAYLIST_DIR` | `C:\Users\<you>\Music\Playlists` |

---

## Advanced / Manual Use

The individual scripts still work if you prefer to run steps directly:

| Script | What it does |
|---|---|
| `1_setup_sldl.ps1` | Downloads and installs Sockseek |
| `2_prep_csvs.py` | Converts Exportify CSVs to Sockseek format |
| `3_download_all.ps1` | Runs Sockseek against all playlists (edit creds at top) |
| `4_generate_m3u.py` | Generates M3U playlist files |

---

## Notes

- **Use a VPN:** Soulseek is peer-to-peer — your real IP address is visible to every user
  you download from. Connect to a VPN before starting downloads.
  [Mullvad](https://mullvad.net) and [ProtonVPN](https://protonvpn.com) are popular
  privacy-focused options that work well with P2P traffic.
- **Rate limits:** Sockseek searches Soulseek at a conservative rate by default. Pushing
  `--searches-per-time` too high can result in temporary 30-minute bans.
- **Niche tracks:** Obscure songs may not be available on Soulseek. Check `m3u_unmatched.txt`
  after generating M3Us and source those manually (Bandcamp, direct purchase, etc.).
- **yt-dlp fallback:** Sockseek supports `--yt-dlp` to fall back to YouTube for tracks not
  found on Soulseek. Requires [yt-dlp](https://github.com/yt-dlp/yt-dlp) on your PATH.
- **Re-running:** All steps are safe to re-run. Downloads skip existing files; M3Us are
  fully regenerated each time.

---

## License

MIT — do whatever you want with it.
