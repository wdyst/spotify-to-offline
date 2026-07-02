# spotify-to-offline  (`s2o`)

Move your Spotify library to a local FLAC collection and generate DAP-ready M3U playlists —
all from a single self-contained binary.

```
s2o setup      # first-time config wizard
s2o import     # pull in your Exportify CSVs
s2o ui         # launch the TUI to download + monitor
s2o m3u        # generate M3U playlists for your DAP
```

---

## What it does

1. **Import** — reads your [Exportify](https://exportify.net) playlist CSVs and stores them in a local SQLite DB
2. **Download** — fetches every track via Soulseek (`sldl`) and/or yt-dlp, concurrently, with live TUI progress
3. **Tag** — writes correct artist/title/album tags to every downloaded file using your Exportify metadata
4. **M3U** — fuzzy-matches your library and generates relative-path playlist files that work on any DAP or SD card

---

## Install

### One-liner (recommended)

**Windows** — run in PowerShell:
```powershell
irm https://raw.githubusercontent.com/kadokonkwo/spotify-to-offline/main/install.ps1 | iex
```

**Linux / macOS** — run in your terminal:
```bash
curl -fsSL https://raw.githubusercontent.com/kadokonkwo/spotify-to-offline/main/install.sh | bash
```

This downloads the latest pre-built binary, copies it to a permanent location, and adds it to your PATH. Open a new terminal and `s2o` is ready.

### Manual install

1. Download the binary for your platform from [Releases](../../releases):
   - `s2o-windows-x64.exe`
   - `s2o-linux-x64`
   - `s2o-macos-x64` / `s2o-macos-arm64`
2. Run it once with the `install` subcommand:
   ```
   s2o-windows-x64.exe install   # Windows
   ./s2o-linux-x64 install       # Linux / macOS
   ```
   This copies the binary to `%LOCALAPPDATA%\s2o\bin\` (Windows) or `~/.local/bin/` (Linux/macOS) and adds it to your PATH automatically.

### Build from source

```bash
git clone https://github.com/kadokonkwo/spotify-to-offline
cd spotify-to-offline
cargo build --release
.\target\release\s2o.exe install   # Windows
./target/release/s2o install       # Linux / macOS
```

Requires Rust + a C toolchain (MinGW on Windows, GCC/Clang on Linux/macOS). The release binary is fully self-contained — no Rust or runtime needed on the target machine.

### Download provider requirements

- **Soulseek**: free account at [slsknet.org](https://www.slsknet.org) + [slsk-batchdl](https://github.com/fiso64/slsk-batchdl) — drop `sldl.exe` / `sockseek.exe` next to `s2o.exe` or configure its path in Settings
- **yt-dlp** *(optional fallback)*: [yt-dlp](https://github.com/yt-dlp/yt-dlp) on your PATH
- **VPN** *(Soulseek only)*: Soulseek is P2P — [Mullvad](https://mullvad.net) or [ProtonVPN](https://protonvpn.com) recommended

---

## Quick start

```
1. Export your Spotify playlists at exportify.net  →  "Export All"  →  save the ZIP somewhere
2. s2o setup           ← one-time wizard: paths, credentials, provider order
3. s2o import <zip>    ← drag the ZIP path in, or pass it as an argument
4. s2o ui              ← TUI: select playlists, hit Enter, watch it go
5. s2o m3u             ← generates M3U files for your DAP
```

---

## TUI

`s2o ui` opens the interactive download interface:

```
╭─ Playlists ──────────╮╭─ Log ─────────────────────────────────────────────╮
│ ▶ pop_punk           ││ [12:01] ━━ Starting 3 playlist(s)…                │
│   Liked_Songs        ││ [12:01] ▶ pop_punk (1/3)                          │
│   vibes              ││ [12:02]   ✓ blink-182 — Dammit [flac]             │
│                      ││ [12:02]   ✓ Sum 41 — Fat Lip [flac]               │
│                      ││ [12:02]   ⚠ AFI — Miss Murder [quality: mp3≠flac] │
│                      ││ [12:03]   ✗ rare track — not found                │
╰──────────────────────╯╰───────────────────────────────────────────────────╯
╭─ Progress ──────────────────────────────────────────╮
│ ██████████████████░░░░  pop_punk  78%               │
╰─────────────────────────────────────────────────────╯
 ↑↓ navigate   Enter download   a all   s settings   q quit
```

Playlist colors: **green** = fully on disk · **yellow** = partially downloaded · **red** = attempted, nothing on disk · **white** = never downloaded. Each playlist shows its `on-disk/total` track count.

Keys: `Enter` download · `a` all · `x` cancel · `/` search · `d` delete · `i` import · `m` M3U · `s` settings (works mid-download) · `r` rescan · `l` save log · `?` full help.

Tracks already in your library (from any playlist) are detected and **reused instead of re-downloaded** — the log reports "N/M tracks already on disk".

---

## Commands

| Command | Description |
|---|---|
| `s2o setup` | First-time configuration wizard |
| `s2o ui` | Interactive TUI (download + monitor) |
| `s2o import [zip]` | Import Exportify CSVs (ZIP or folder) |
| `s2o download [-p playlist]` | Headless download (omit name = all playlists) |
| `s2o m3u [profile]` | Generate M3U files (optional DAP profile override) |
| `s2o status` | Per-playlist on-disk counts + library totals |
| `s2o remove <playlist>` | Remove a playlist (CSVs, M3U, history — audio kept) |

---

## Configuration

Run `s2o setup` once to configure, or press `s` inside the TUI to edit live. Config lives at:

- **Windows**: `%APPDATA%\s2o\config.toml`
- **Linux/macOS**: `~/.config/s2o/config.toml`

(Or drop a `config.toml` next to `s2o.exe` to make it portable.)

### Key settings

| Setting | Default | Description |
|---|---|---|
| `paths.music_root` | `~/Music` | Where downloads land |
| `paths.playlists_dir` | `~/Music/Playlists` | Where M3U files are written |
| `paths.sockseek_path` | *(auto-detect)* | Path to `sldl.exe` / `sockseek.exe` |
| `paths.ytdlp_path` | `yt-dlp` | Command or path for yt-dlp |
| `soulseek.username` | — | Soulseek username |
| `soulseek.password` | — | Soulseek password |
| `provider.order` | `["soulseek","ytdlp"]` | Try providers in this order |
| `provider.fallback_enabled` | `true` | Fall back to next provider if track not found |
| `download.preferred_format` | `flac` | Preferred audio format (passed to sldl as `--pref-format`) |
| `download.name_format` | `{sartist}/{salbum}/{stitle}` | How downloaded files are named/organised (sldl `--name-format`) |
| `download.quality_warning` | `true` | Notify + log when format doesn't match |
| `download.concurrent_playlists` | `2` | Playlists downloading at the same time |

---

## DAP profiles

`s2o m3u` generates relative-path M3U files tuned for your player. Built-in profiles:

| Profile | Description |
|---|---|
| `universal` | Relative paths, works on most players |
| `snowsky` | Tuned for the Snowsky Echo Mini |
| `fiio` | FiiO players (tested on M11) |
| `hiby` | HiBy players |
| `shanling` | Shanling players |

Set your default in `setup` or pass `--profile <name>` to `s2o m3u`.

---

## Output structure

```
Music/
├── Artist Name/
│   └── Album Name/
│       └── Track Title.flac
└── Playlists/
    ├── pop_punk.m3u
    ├── Liked_Songs.m3u
    └── ...              ← relative paths, DAP-ready
```

---

## Soulseek binary auto-detection

s2o looks for the slsk-batchdl binary in this order:

1. `sockseek.exe` / `sldl.exe` next to `s2o.exe`
2. `sockseek` / `sldl` on your PATH
3. The path set in `paths.sockseek_path` (config or Settings)

Either name works — `sldl` is the current official name; `sockseek` is the legacy name.

---

## Notes

- **Resuming**: all downloads are tracked in `s2o.db` — re-running skips already-found tracks
- **Quality warnings**: if `download.quality_warning = true`, any track that lands in a lower format than requested gets logged and optionally sends a desktop notification
- **Niche tracks**: check `s2o status` for not-found tracks — source those via Bandcamp, direct purchase, or yt-dlp fallback
- **Re-running M3U**: always safe; rescans the library and regenerates everything

---

## Python version (legacy)

The original Python scripts are preserved in [`python_legacy/`](python_legacy/) for reference.
They are no longer maintained. The Rust binary covers everything they did and more.

---

## License

MIT
