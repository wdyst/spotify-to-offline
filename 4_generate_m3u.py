r"""
4_generate_m3u.py -- Generate M3U playlists for the Snowsky Echo Mini.
Run AFTER sldl has finished downloading (3_download_all.ps1).

How it works:
  1. Scans all audio files in C:\Users\kado\Music\ (recursive)
  2. Builds a fuzzy search index by artist + title
  3. Reads each playlist CSV and matches tracks to local files
  4. Writes M3U files to C:\Users\kado\Music\Playlists\

M3U paths are relative (../Artist/Album/title.flac) so the Playlists\
folder and music folders can be copied together to the Snowsky SD card
and everything works without editing paths.
"""

import csv, os, re, glob, unicodedata, sys
from difflib import SequenceMatcher
sys.stdout.reconfigure(encoding='utf-8')

# ---------------------------------------------------------------------------
# CONFIGURATION — edit these if your music lives somewhere other than ~/Music
# ---------------------------------------------------------------------------
_USERMUSIC   = os.path.join(os.path.expanduser("~"), "Music")
MUSIC_ROOT   = os.environ.get("MUSIC_ROOT",   _USERMUSIC)
PLAYLIST_DIR = os.environ.get("PLAYLIST_DIR", os.path.join(MUSIC_ROOT, "Playlists"))

_HERE    = os.path.dirname(os.path.abspath(__file__))
CSV_DIR  = os.path.join(_HERE, "playlists_sldl")
LOG_PATH = os.path.join(_HERE, "m3u_unmatched.txt")
# ---------------------------------------------------------------------------

AUDIO_EXTS = {'.flac', '.mp3', '.m4a', '.wav', '.ogg', '.opus', '.aac'}

os.makedirs(PLAYLIST_DIR, exist_ok=True)

# ─── 1. Scan library ──────────────────────────────────────────────────────────

def normalize(s):
    """Lowercase, strip accents, remove punctuation/articles for matching."""
    s = unicodedata.normalize('NFKD', s).encode('ascii', 'ignore').decode()
    s = s.lower()
    s = re.sub(r"^(the |a |an )", "", s)     # strip leading articles
    s = re.sub(r"[^\w\s]", " ", s)            # punctuation → space
    s = re.sub(r"\s+", " ", s).strip()
    return s

def strip_track_num(name):
    """Remove leading track numbers like '01 - ', '1. ' from filenames."""
    return re.sub(r"^\d+\s*[-\.]\s*", "", name).strip()

print("Scanning music library...")
library = []   # list of dicts: {path, title_norm, artist_norm, rel_path}

for root, dirs, files in os.walk(MUSIC_ROOT):
    # Skip the Playlists and spotify_tools folders themselves
    dirs[:] = [d for d in dirs if d not in ('Playlists', 'spotify_tools')]
    for fname in files:
        ext = os.path.splitext(fname)[1].lower()
        if ext not in AUDIO_EXTS:
            continue
        full_path = os.path.join(root, fname)
        rel_path  = os.path.relpath(full_path, PLAYLIST_DIR)  # relative from Playlists\
        base_name = os.path.splitext(fname)[0]
        clean     = strip_track_num(base_name)

        # Infer artist from folder structure (grandparent of file)
        parts = full_path.replace(MUSIC_ROOT, '').lstrip('\\').split('\\')
        inferred_artist = parts[0] if len(parts) >= 2 else ''

        library.append({
            'path':         full_path,
            'rel_path':     rel_path.replace('\\', '/'),   # forward slashes for M3U
            'title_norm':   normalize(clean),
            'artist_norm':  normalize(inferred_artist),
            'base_name':    clean,
        })

print(f"  Found {len(library)} audio files\n")

# ─── 2. Build lookup indexes ──────────────────────────────────────────────────

# Fast exact-match dict: (title_norm, artist_norm) → entry
exact_index = {}
for entry in library:
    key = (entry['title_norm'], entry['artist_norm'])
    if key not in exact_index:
        exact_index[key] = entry

# Title-only index for when artist doesn't match folder name
title_index = {}
for entry in library:
    key = entry['title_norm']
    if key not in title_index:
        title_index[key] = []
    title_index[key].append(entry)

def similarity(a, b):
    return SequenceMatcher(None, a, b).ratio()

def find_file(title, artist):
    tn = normalize(title)
    an = normalize(artist)

    # 1. Exact title + artist match
    hit = exact_index.get((tn, an))
    if hit:
        return hit, 'exact'

    # 2. Exact title, any artist
    candidates = title_index.get(tn, [])
    if len(candidates) == 1:
        return candidates[0], 'title-only'
    if len(candidates) > 1:
        # Pick closest artist
        best = max(candidates, key=lambda e: similarity(e['artist_norm'], an))
        return best, 'title+fuzzy-artist'

    # 3. Fuzzy title match across all library entries
    best_score = 0.0
    best_entry = None
    for entry in library:
        score = similarity(entry['title_norm'], tn)
        if score > best_score:
            best_score = score
            best_entry = entry
    if best_score >= 0.82:
        return best_entry, f'fuzzy({best_score:.2f})'

    return None, 'not-found'

# ─── 3. Generate M3U files ────────────────────────────────────────────────────

csvs = [f for f in glob.glob(os.path.join(CSV_DIR, "*.csv"))
        if not os.path.basename(f).startswith("00_")]
csvs.sort()

print(f"Generating M3U files for {len(csvs)} playlists...")

unmatched_log = []
stats = {'matched': 0, 'unmatched': 0, 'playlists': 0}

for csv_path in csvs:
    playlist_name = os.path.splitext(os.path.basename(csv_path))[0]
    m3u_path      = os.path.join(PLAYLIST_DIR, playlist_name + ".m3u")

    rows = []
    with open(csv_path, newline='', encoding='utf-8') as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            rows.append(row)

    if not rows:
        continue

    matched = 0
    lines   = ["#EXTM3U", f"#PLAYLIST:{playlist_name}"]

    for row in rows:
        title    = row.get('Title', '').strip()
        artist   = row.get('Artist', '').strip()
        duration = row.get('Length', '-1').strip()

        entry, method = find_file(title, artist)

        if entry:
            lines.append(f"#EXTINF:{duration},{artist} - {title}")
            lines.append(entry['rel_path'])
            matched += 1
            stats['matched'] += 1
        else:
            # Leave a commented-out stub so the playlist order is preserved
            lines.append(f"#EXTINF:{duration},{artist} - {title}")
            lines.append(f"#MISSING: {artist} - {title}")
            stats['unmatched'] += 1
            unmatched_log.append(f"{playlist_name}\t{artist}\t{title}")

    with open(m3u_path, 'w', encoding='utf-8') as fh:
        fh.write('\n'.join(lines) + '\n')

    pct = int(100 * matched / len(rows)) if rows else 0
    print(f"  {playlist_name}: {matched}/{len(rows)} matched ({pct}%)")
    stats['playlists'] += 1

# ─── 4. Write unmatched log ───────────────────────────────────────────────────

if unmatched_log:
    with open(LOG_PATH, 'w', encoding='utf-8') as fh:
        fh.write("PLAYLIST\tARTIST\tTITLE\n")
        fh.write('\n'.join(unmatched_log))
    print(f"\nUnmatched tracks logged to: {LOG_PATH}")

print(f"\n{'='*50}")
print(f"Done! {stats['playlists']} playlists written to: {PLAYLIST_DIR}")
print(f"  Matched:   {stats['matched']} tracks")
print(f"  Unmatched: {stats['unmatched']} tracks  ← check m3u_unmatched.txt")
print(f"\nSnowsky setup: copy your music folders + Playlists\\ to the SD card root.")
