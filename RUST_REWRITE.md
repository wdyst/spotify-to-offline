# Rust Rewrite — Status & Roadmap

The Python implementation (`run.py`, `2_prep_csvs.py`, `4_generate_m3u.py`) is fully functional.
This document tracks progress on the optional Rust port.

## Why Rust

- Single binary — no Python runtime dependency
- Faster startup and lower memory footprint
- Cross-platform without worrying about `python` vs `python3`
- Fun

## Binary name

`s2o` — build with `cargo build --release`, run with `./s2o` or `s2o.exe`

## Status

| Component              | Python file          | Rust file             | Status         |
|------------------------|----------------------|-----------------------|----------------|
| Config (read/write)    | `run.py`             | `src/config.rs`       | 🟡 Stub        |
| Interactive menu       | `run.py`             | `src/ui.rs`           | 🟡 Stub        |
| CSV conversion         | `2_prep_csvs.py`     | —                     | ⬜ Not started |
| Soulseek provider      | `run.py`             | `src/providers.rs`    | 🟡 Stub        |
| yt-dlp provider        | `run.py`             | `src/providers.rs`    | 🟡 Stub        |
| Custom provider        | `run.py`             | `src/providers.rs`    | 🟡 Stub        |
| M3U generation         | `4_generate_m3u.py`  | —                     | ⬜ Not started |

## Build

```bash
cargo build --release
./target/release/s2o          # Linux/macOS
.\target\release\s2o.exe      # Windows
```

## Contributing

The Python code is the reference implementation — port logic 1:1 where possible.
Config uses INI format (`config.ini`) on both sides to stay compatible.
