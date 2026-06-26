# Rust Rewrite — Action Plan & Feature Proposal

> **How to use this doc:** Review each section and mark items ✅ yes / ❌ no / 🔄 later.
> Send it back and I'll implement everything approved in one go.

---

## Why bother rewriting at all

The Python version works. The rewrite is worth doing for one concrete reason:
**single binary distribution** — `s2o.exe` / `s2o` that anyone can run with no runtime
dependency. Everything else is a bonus. That said, the rewrite is also an opportunity to
add features that would be awkward to bolt onto the Python version.

---

## Architecture decisions  *(these affect everything — decide first)*

### A. Config format: TOML instead of INI

`config.toml` instead of `config.ini`. TOML is the standard for Rust tools, has better
type support (arrays, nested tables), and is more readable. Not compatible with the
Python version's `config.ini`, so users would need to re-enter settings once on first run.
The Rust binary would detect and migrate an existing `config.ini` automatically.

**Proposed config structure:**
```toml
[soulseek]
username = "ohwhattheheck"
password = "..."

[paths]
music_root    = "C:/Users/kado/Music"
playlists_dir = "C:/Users/kado/Music/Playlists"

[provider]
order = ["soulseek", "ytdlp"]   # fallback chain — see Feature 3

[[dap_profiles]]                  # see Feature 9
name      = "snowsky"
music_root = "E:/"
m3u_dir   = "E:/Playlists"
```

### B. Interface: TUI (recommended) vs plain CLI

**Option 1 — ratatui TUI:** Live multi-pane terminal UI. Left pane shows playlists
queued/in-progress/done; right pane shows the current download stream; bottom bar shows
overall progress. Looks great, gives you real information at a glance.

**Option 2 — plain CLI:** Same look as the Python version — colored text, menus, line
output. Much simpler to build. Basically a 1:1 port.

*Recommendation: TUI. The download phase is where you spend hours staring at the terminal
— good feedback matters. ratatui is also one of Rust's most mature crates.*

### C. Async runtime: tokio

Required for concurrent downloads (Feature 5) and watch mode (Feature 8). Adds some
complexity but opens the door to downloading multiple playlists in parallel, which cuts
hours off a large library sync. All provider calls become async.

---

## Core features  *(direct ports — these are the baseline, all should be yes)*

| # | Feature | Notes |
|---|---------|-------|
| C1 | TOML config read/write | Replaces configparser |
| C2 | Exportify ZIP import | Drag-and-drop or path arg |
| C3 | CSV conversion | Exportify → sockseek format |
| C4 | Soulseek provider (via sockseek) | Same flags as Python version |
| C5 | yt-dlp provider | Per-track search + FLAC download |
| C6 | Custom command provider | `{artist}` `{title}` `{album}` `{output}` |
| C7 | M3U generation | Fuzzy match via `strsim` crate |
| C8 | Settings menu / subcommand | All path/provider config |
| C9 | Cross-platform | Windows / Linux / macOS |
| C10 | Single binary, no Python needed | The whole point |


---

## Suggested new features

---

### Feature 1 — Download state database  *(high value)*

**What:** A local SQLite database (`s2o.db`) that remembers every download attempt:
track, provider used, outcome (success / not found / failed), file path, timestamp.

**Why it matters:** The Python version has no memory — it determines "already downloaded"
purely by checking if a file exists. This means:
- You can't tell which tracks failed vs were never attempted
- You can't retry just the failures
- You have no idea what percentage of your library actually made it

With a state DB you get:
- `s2o status` — how many tracks downloaded, how many failed, success rate per playlist
- `s2o retry` — re-attempt only failed tracks, skipping everything that worked
- The foundation for delta sync (Feature 2)

**Crate:** `rusqlite` (SQLite, zero-server, single file)

---

### Feature 2 — Playlist delta sync  *(high value)*

**What:** When you run a download after the initial sync, only download tracks that
were *added to the playlist since the last sync* — not the entire playlist again.

**Why it matters:** You will keep adding songs to Spotify playlists over time. Currently
re-processing a 200-track playlist to get 3 new songs means Sockseek has to check all
200 against your library. With delta sync + state DB it skips straight to the 3 new ones.

**How:** Exportify CSVs include `Added At` timestamps. State DB records the last sync
time per playlist. On next run, only rows with `Added At` > last sync time are processed.

**Requires:** Feature 1 (state DB)

---

### Feature 3 — Provider fallback chain  *(high value)*

**What:** Instead of one provider, define an ordered list. The app tries each provider
in sequence for tracks not found by the previous one.

**Example config:**
```toml
[provider]
order = ["soulseek", "ytdlp"]
```

Soulseek handles ~95% of tracks. yt-dlp fills the gaps for obscure stuff that isn't
shared on the network. Right now you'd have to manually identify failures and re-run
with a different provider. This makes it automatic.

**How it works:** After a full Soulseek pass, tracks marked "not found" in the state DB
get a second pass via yt-dlp. You get one clean run that maximises coverage.

**Requires:** Feature 1 (state DB, to track "not found" status)

---

### Feature 4 — Tag normalization after download  *(medium value)*

**What:** After downloading a file, write clean ID3v2/FLAC Vorbis tags from the Spotify
metadata you already have in the CSV (artist, album, title, release year).

**Why it matters:** Soulseek files come from random people's libraries. Tags are often
wrong, missing, inconsistently formatted, or in a different encoding. Your DAP's
"browse by artist" and "browse by album" views depend entirely on tags. Normalizing
them means your library actually looks right on the Snowsky.

**What gets written:**
- `ARTIST`, `ALBUM`, `TITLE` — from CSV
- `DATE` / `YEAR` — from `Release Date` column
- `TRACKNUMBER` — from playlist order
- `COMMENT` — "Downloaded by spotify-to-offline"

**Crate:** `lofty` (unified tag read/write for FLAC, MP3, M4A, OGG)

---

### Feature 5 — Concurrent downloads  *(medium value)*

**What:** Download multiple playlists (or tracks, for yt-dlp) in parallel via tokio
async tasks, with a configurable concurrency limit.

**Example config:**
```toml
[download]
concurrent_playlists = 2   # sockseek instances running simultaneously
concurrent_tracks    = 4   # yt-dlp tracks in parallel (per playlist)
```

**Why it matters:** Sequential playlist downloads with Sockseek means playlist 50 can't
start until playlist 49 finishes. With 2 concurrent instances you roughly halve wall
time. The Soulseek network handles parallel sessions fine.

**Caveat:** Sockseek itself already does concurrent *searches* within one playlist.
The gain here is parallelising across *playlists*, which is still significant.

**Requires:** Architecture decision C (tokio async)

---

### Feature 6 — Library health report  *(medium value)*

**What:** `s2o doctor` — scans your music library and reports:
- Tracks in your playlist CSVs that have no matching local file
- Files with missing or empty tags
- Duplicate files (same artist+title in multiple locations)
- FLAC files that fail integrity check (`flac -t` equivalent)
- M3U files that reference paths that no longer exist

Output as a formatted table to terminal, optionally exported to `health_report.txt`.

**Why it matters:** After downloading thousands of tracks across months, your library
accumulates cruft. This gives you one command to assess its state.

---

### Feature 7 — Notifications on completion  *(low effort, nice to have)*

**What:** Fire a system notification when a long download batch finishes, so you can
leave it running and get pinged when it's done.

- Windows: Windows Toast notifications
- Linux: `notify-send`
- macOS: `osascript` notification

**Crate:** `notify-rust` handles all three platforms.

---

### Feature 8 — Watch mode  *(medium value)*

**What:** `s2o watch` — monitor a folder (e.g. your Downloads folder) for new
Exportify ZIPs and automatically trigger import → download → M3U when one appears.

Set it up once in the background. Export from Exportify, walk away. Come back to a
synced library.

**Config:**
```toml
[watch]
folder      = "C:/Users/kado/Downloads"
auto_m3u    = true
auto_notify = true
```

**Crate:** `notify` (cross-platform filesystem events)

**Requires:** Architecture decision C (tokio async, for running the watcher loop)

---

### Feature 9 — Multiple DAP profiles  *(medium value)*

**What:** Define multiple output profiles for different devices. Each profile has its
own music root, playlists directory, and M3U path format.

**Why it matters:** You might have the Snowsky in your pocket and a different DAP at
home, or use a different SD card for car vs. home listening. Currently you'd need to
manually change Settings between uses.

**Config:**
```toml
[[dap_profiles]]
name       = "snowsky"
music_root = "E:/"
m3u_dir    = "E:/Playlists"
path_style = "relative"   # relative | absolute

[[dap_profiles]]
name       = "car_sd"
music_root = "F:/"
m3u_dir    = "F:/Playlists"
path_style = "relative"
```

`s2o m3u --profile snowsky` generates for that device specifically.
`s2o m3u --all-profiles` generates for all of them at once.

---

### Feature 10 — Spotify API direct import  *(high value, more work)*

**What:** Skip Exportify entirely. Authenticate directly with Spotify's API (PKCE OAuth
flow, no server or client secret needed) and pull playlist data in-app.

**Why it matters:** Exportify requires opening a browser, logging in, clicking Export All,
waiting for a ZIP, dragging it into the terminal. Direct API access collapses this to
`s2o import` — done. Also enables delta sync (Feature 2) without needing a new Exportify
export every time, because you can just query "what tracks were added since X date."

**What's involved:**
- PKCE OAuth flow (opens browser for one-time auth, stores token)
- Token refresh (transparent, happens automatically)
- Playlist + track list API calls
- Produces the same internal format as Exportify CSV

**Crate:** `oauth2` + `reqwest`

**Note:** This is the biggest single feature in terms of effort. Worth it if you plan to
keep the tool long-term — eliminates the most tedious manual step.

---

### Feature 11 — Format preferences per playlist  *(lower priority)*

**What:** Override the download format on a per-playlist basis via a config file or
special playlist naming convention.

**Example use case:** Your "Workout" playlist → MP3 320k (smaller, faster seek on some
players). Your "Audiophile" playlist → FLAC only, refuse MP3 fallback.

```toml
[[playlist_overrides]]
name   = "Workout"
format = "mp3"
quality = "320"

[[playlist_overrides]]
name   = "Audiophile Picks"
format = "flac"
strict = true   # fail track rather than fall back to lossy
```

---

---

## Crate stack

| Purpose | Crate | Notes |
|---------|-------|-------|
| CLI args / subcommands | `clap` v4 | derive macro, best-in-class |
| TUI | `ratatui` | if Architecture B = TUI |
| Async runtime | `tokio` | if Architecture C = async |
| Config (TOML) | `serde` + `toml` | |
| State database | `rusqlite` | SQLite, no server |
| HTTP | `reqwest` | downloads, Spotify API |
| OAuth | `oauth2` | Spotify login (Feature 10) |
| Audio tags | `lofty` | unified FLAC/MP3/M4A/OGG |
| Fuzzy string match | `strsim` | replaces Python difflib |
| Filesystem watch | `notify` | cross-platform (Feature 8) |
| System notifications | `notify-rust` | Windows/Linux/macOS (Feature 7) |
| Progress bars | `indicatif` | if plain CLI; ratatui handles it in TUI mode |
| Error handling | `anyhow` | already in scaffold |
| Platform dirs | `dirs` | `~/Music` cross-platform |
| CSV parsing | `csv` | already in scaffold |

---

## Phased roadmap

### Phase 1 — Foundation  *(get it compiling and usable)*
- Config read/write (TOML), migration from `config.ini`
- CLI skeleton with all subcommands wired up
- Exportify ZIP import + CSV conversion
- Settings management
- Basic provider dispatch (call sockseek/yt-dlp as subprocess)
- M3U generation with fuzzy matching

*End state: feature-parity with Python version as a single binary*

### Phase 2 — Core quality-of-life
- TUI (ratatui) — live progress, playlist queue view
- Concurrent downloads (tokio + semaphore)
- Provider fallback chain (Feature 3)
- Tag normalization (Feature 4)
- Completion notifications (Feature 7)

### Phase 3 — Smart sync
- State database (Feature 1)
- Delta sync (Feature 2)
- Library health report (Feature 6)

### Phase 4 — Advanced
- Watch mode (Feature 8)
- Multiple DAP profiles (Feature 9)
- Spotify API direct import (Feature 10)
- Per-playlist format preferences (Feature 11)

---

## What's deliberately excluded

| Idea | Why not |
|------|---------|
| Web UI | Overkill for a personal CLI tool |
| Built-in SLSK protocol | That's basically rewriting sockseek; not worth it while sockseek exists |
| Bandcamp integration | Different problem, different tool |
| MusicBrainz deep lookup | Complex, slow, often wrong for niche stuff; CSV metadata is more reliable |
| Torrent sources | Legal complexity, out of scope |

---

## Current scaffold status

| File | Status |
|------|--------|
| `Cargo.toml` | ✅ Crates listed |
| `src/main.rs` | 🟡 Subcommands wired, all `todo!()` |
| `src/config.rs` | 🟡 Stub |
| `src/providers.rs` | 🟡 Trait defined, no implementations |
| `src/ui.rs` | 🟡 Stub |

> Mark up this document and send it back — anything approved goes straight into implementation.
