# spotify-to-offline

Move your Spotify playlists to a local FLAC library, then generate M3U playlist files ready for a portable music player (tested on Snowsky Echo Mini).

**What it does:**
1. Reads your [Exportify](https://exportify.net) CSV exports
2. Converts them to a format [Sockseek](https://github.com/fiso64/slsk-batchdl) (formerly sldl) can consume
3. Batch-downloads every track from Soulseek, preferring FLAC, skipping anything you already own
4. Generates M3U playlist files with relative paths so they work on any DAP or media player

---

## Requirements

- **Windows** (scripts use PowerShell; the Python files work cross-platform)
- **Python 3.8+** — no third-party packages needed (stdlib only)
- **A Soulseek account** — free at [slsknet.org](https://www.slsknet.org)
- **Your Spotify playlists exported via [Exportify](https://exportify.net)** — log in with Spotify, export all playlists as a ZIP of CSVs

---

## Quick Start

```powershell
# 1. Clone the repo somewhere convenient
git clone https://github.com/wdyst/spotify-to-offline
cd spotify-to-offline

# 2. Extract your Exportify ZIP (adjust path to wherever you saved it)
Expand-Archive -Path "C:\path\to\spotify_playlists.zip" -DestinationPath ".\playlists_raw" -Force

# 3. Install Sockseek
powershell -ExecutionPolicy Bypass -File "1_setup_sldl.ps1"

# 4. Convert CSVs
python "2_prep_csvs.py"

# 5. Edit credentials, then download (takes hours for large libraries)
notepad "3_download_all.ps1"   # fill in SlskUser / SlskPass at the top
powershell -ExecutionPolicy Bypass -File "3_download_all.ps1"

# 6. After downloading, generate M3U playlists
python "4_generate_m3u.py"
```

---

## Detailed Steps

### Step 1 — Export from Spotify

1. Go to [exportify.net](https://exportify.net) and sign in with Spotify
2. Click **Export All** to download a ZIP of CSVs (one file per playlist)
3. Save the ZIP somewhere you can find it

### Step 2 — Set up the tools

```powershell
# Extract your Exportify ZIP into playlists_raw\
Expand-Archive -Path "C:\path\to\spotify_playlists.zip" -DestinationPath ".\playlists_raw" -Force

# Download and install Sockseek (the Soulseek batch downloader)
powershell -ExecutionPolicy Bypass -File "1_setup_sldl.ps1"
```

### Step 3 — Convert playlist CSVs

```powershell
python 2_prep_csvs.py
```

This reads every CSV from `playlists_raw\`, renames columns to what Sockseek expects,
converts duration from milliseconds to seconds, and writes cleaned CSVs to `playlists_sldl\`.
It also produces `playlists_sldl\00_all_tracks.csv` — a deduplicated master list of every
unique track across all your playlists.

### Step 4 — Download

Open `3_download_all.ps1` in any text editor and fill in your credentials at the top:

```powershell
$SlskUser = "your_soulseek_username"
$SlskPass = "your_soulseek_password"
```

Then run it:

```powershell
powershell -ExecutionPolicy Bypass -File "3_download_all.ps1"
```

> **⚠ Connect to a VPN first.** Soulseek is P2P and your real IP is visible to every peer
> you download from. [Mullvad](https://mullvad.net) and [ProtonVPN](https://protonvpn.com)
> are solid choices that don't throttle P2P traffic.

Downloads go to `C:\Users\<you>\Music\{Artist}\{Album}\{Title}.flac` by default.
Edit `$MusicRoot` in the script to change the destination.

**You can stop and restart at any time** — Sockseek tracks what's already downloaded and skips those files.

Sockseek prefers FLAC but falls back to MP3/M4A if no lossless copy is available on the network.
It also cross-references your existing music library and skips tracks you already own.

### Step 5 — Generate M3U playlists

```powershell
python 4_generate_m3u.py
```

Scans your music library, fuzzy-matches each playlist track to a local file, and writes
one `.m3u` file per playlist to `C:\Users\<you>\Music\Playlists\`.

M3U paths are **relative** (e.g. `../Artist/Album/title.flac`) so the playlist files
work correctly whether you're on your PC or on a DAP's SD card.

Any tracks that couldn't be matched are logged to `m3u_unmatched.txt` for review.

Re-run this script any time you add more music — it always reflects your current library.

---

## Output Structure

```
Music\
├── Artist Name\
│   └── Album Name\
│       └── Track Title.flac        <- new downloads
├── blink-182\                       <- existing library (any structure works)
│   └── ...
└── Playlists\
    ├── Liked_Songs.m3u
    ├── pop_punk.m3u
    └── ...                         <- one file per playlist, Snowsky-ready
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

All paths are set at the top of each script. Key variables:

| Script | Variable | Default |
|---|---|---|
| `3_download_all.ps1` | `$MusicRoot` | `C:\Users\<you>\Music` |
| `3_download_all.ps1` | `$PlaylistsDir` | `$MusicRoot\Playlists` |
| `4_generate_m3u.py` | `MUSIC_ROOT` | `C:\Users\<you>\Music` |
| `4_generate_m3u.py` | `PLAYLIST_DIR` | `C:\Users\<you>\Music\Playlists` |

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
- **Re-running:** Both `3_download_all.ps1` and `4_generate_m3u.py` are safe to re-run.
  Downloads skip existing files; M3Us are fully regenerated each time.

---

## License

MIT — do whatever you want with it.
