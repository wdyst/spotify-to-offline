"""
2_prep_csvs.py -- Convert Exportify CSVs to Sockseek-compatible format.

Run after extracting your Exportify ZIP into playlists_raw/ (see README).

Input:  playlists_raw/**/*.csv           (Exportify format)
Output: playlists_sldl/*.csv             (Sockseek-ready, correct column names)
        playlists_sldl/00_all_tracks.csv (deduplicated master track list)
"""

import csv, os, re, glob, sys
sys.stdout.reconfigure(encoding='utf-8')  # handle emoji playlist names on Windows

# Paths are relative to this script's location so the repo is portable
_HERE    = os.path.dirname(os.path.abspath(__file__))
RAW_DIR  = os.path.join(_HERE, "playlists_raw")
OUT_DIR  = os.path.join(_HERE, "playlists_sldl")
os.makedirs(OUT_DIR, exist_ok=True)

def find_csvs(base):
    """Find all CSVs under base, regardless of subfolder depth."""
    return glob.glob(os.path.join(base, "**", "*.csv"), recursive=True)

def clean_name(s):
    """Sanitize a playlist name for use as a filename."""
    s = re.sub(r'[\\/:*?"<>|]', '_', s)  # Windows-illegal chars
    s = s.strip().strip('.')
    return s[:80] if s else "unnamed"

csvs = find_csvs(RAW_DIR)
if not csvs:
    print(f"ERROR: No CSVs found under {RAW_DIR}")
    print("Did you run the Expand-Archive step from README.txt?")
    raise SystemExit(1)

print(f"Found {len(csvs)} playlist CSVs")

all_tracks = {}     # (title_lower, artist_lower) -> row dict (deduplicated)
playlist_summary = []

for src_path in sorted(csvs):
    playlist_name = os.path.splitext(os.path.basename(src_path))[0]
    safe_name     = clean_name(playlist_name)
    dest_path     = os.path.join(OUT_DIR, safe_name + ".csv")

    rows_out = []
    with open(src_path, newline='', encoding='utf-8') as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            title    = row.get('Track Name', '').strip()
            artists  = row.get('Artist Name(s)', '').strip()
            album    = row.get('Album Name', '').strip()
            dur_ms   = row.get('Duration (ms)', '0').strip()
            uri      = row.get('Track URI', '').strip()
            if not title:
                continue
            # sldl uses first artist when multiple are listed
            first_artist = artists.split(';')[0].strip()
            duration_s   = int(dur_ms) // 1000 if dur_ms.isdigit() else 0

            out_row = {
                'Title':    title,
                'Artist':   first_artist,
                'Album':    album,
                'Length':   duration_s,   # sldl accepts seconds
                'Artists':  artists,      # keep full list for reference
                'URI':      uri,
            }
            rows_out.append(out_row)

            key = (title.lower(), first_artist.lower())
            if key not in all_tracks:
                all_tracks[key] = out_row

    # Write per-playlist CSV
    if rows_out:
        with open(dest_path, 'w', newline='', encoding='utf-8') as fh:
            writer = csv.DictWriter(fh, fieldnames=['Title','Artist','Album','Length','Artists','URI'])
            writer.writeheader()
            writer.writerows(rows_out)
        playlist_summary.append((safe_name, len(rows_out)))
        print(f"  {safe_name}: {len(rows_out)} tracks")
    else:
        print(f"  {safe_name}: EMPTY — skipped")

# Write master deduplicated track list
master_path = os.path.join(OUT_DIR, "00_all_tracks.csv")
with open(master_path, 'w', newline='', encoding='utf-8') as fh:
    writer = csv.DictWriter(fh, fieldnames=['Title','Artist','Album','Length','Artists','URI'])
    writer.writeheader()
    writer.writerows(all_tracks.values())

print(f"\nDone.")
print(f"  {len(playlist_summary)} playlists written to: {OUT_DIR}")
print(f"  {len(all_tracks)} unique tracks in: {master_path}")
print(f"\nNext step: edit 3_download_all.ps1 with your Soulseek credentials, then run it.")
