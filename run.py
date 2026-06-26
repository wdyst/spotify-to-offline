"""
spotify-to-offline -- interactive launcher
Double-click run.bat  OR  run:  python run.py
"""
import os, sys, re, csv, glob, time, getpass
import subprocess, configparser, webbrowser, zipfile
sys.stdout.reconfigure(encoding='utf-8')
os.system('')   # enable ANSI colors on Windows

C,G,Y,R,B,D,X = '\033[96m','\033[92m','\033[93m','\033[91m','\033[1m','\033[2m','\033[0m'

HERE          = os.path.dirname(os.path.abspath(__file__))
CONFIG_FILE   = os.path.join(HERE, 'config.ini')
SOCKSEEK      = os.path.join(HERE, 'sockseek.exe')
MUSIC_ROOT    = os.path.join(os.path.expanduser('~'), 'Music')
PLAYLISTS_DIR = os.path.join(MUSIC_ROOT, 'Playlists')
RAW_DIR       = os.path.join(HERE, 'playlists_raw')
SLDL_DIR      = os.path.join(HERE, 'playlists_sldl')

# ── tiny helpers ──────────────────────────────────────────────────────────────
def ok(m):    print(f"  {G}+{X}  {m}")
def warn(m):  print(f"  {Y}!{X}  {m}")
def err(m):   print(f"  {R}x{X}  {m}")
def info(m):  print(f"     {D}{m}{X}")
def hr():     print(f"  {D}{'─'*44}{X}")
def pause():  input(f"\n  {D}Press Enter to return to menu...{X}")

def banner():
    os.system('cls')
    print(f"""
{C}{B}  +----------------------------------------------+
  |          spotify-to-offline  v1.0            |
  |      Spotify -> FLAC -> Snowsky / DAP        |
  +----------------------------------------------+{X}""")

# ── config ────────────────────────────────────────────────────────────────────
def load_cfg():
    c = configparser.RawConfigParser()   # RawConfigParser: no % interpolation
    if os.path.exists(CONFIG_FILE):
        c.read(CONFIG_FILE, encoding='utf-8')
    return c

def save_cfg(c):
    with open(CONFIG_FILE, 'w', encoding='utf-8') as f:
        c.write(f)

# ── step 1: credentials ───────────────────────────────────────────────────────
def step_credentials():
    cfg = load_cfg()
    cur_user = cfg.get('soulseek', 'username', fallback='')
    cur_pass = cfg.get('soulseek', 'password', fallback='')

    print(f"\n  {B}Soulseek credentials{X}  {D}(free account at slsknet.org){X}")
    hr()
    if cur_user and cur_pass:
        ok(f"Already saved: {C}{cur_user}{X}")
        if input("  Change credentials? [y/N] ").strip().lower() != 'y':
            return cur_user, cur_pass

    username = input("  Username: ").strip()
    password = getpass.getpass("  Password: ")

    if not cfg.has_section('soulseek'):
        cfg.add_section('soulseek')
    cfg.set('soulseek', 'username', username)
    cfg.set('soulseek', 'password', password)
    save_cfg(cfg)
    ok("Saved to config.ini")
    return username, password

# ── step 2: exportify ─────────────────────────────────────────────────────────
def step_exportify():
    existing = glob.glob(os.path.join(RAW_DIR, '**', '*.csv'), recursive=True)
    if existing:
        ok(f"Found {len(existing)} playlist CSVs already")
        if input("  Re-import anyway? [y/N] ").strip().lower() != 'y':
            return True

    print(f"\n  {B}Export your Spotify playlists{X}")
    hr()
    print("  Opening exportify.net in your browser...")
    print(f"  {D}Sign in with Spotify, click Export All, save the ZIP.{X}")
    print()
    webbrowser.open("https://exportify.net")

    zip_path = ''
    while True:
        raw = input("  Drag the ZIP file into this window (or paste path): ").strip().strip('"').strip("'")
        if os.path.isfile(raw) and raw.lower().endswith('.zip'):
            zip_path = raw
            break
        err("Not a valid ZIP file. Try again.")

    print("  Extracting...")
    os.makedirs(RAW_DIR, exist_ok=True)
    with zipfile.ZipFile(zip_path, 'r') as z:
        z.extractall(RAW_DIR)
    count = len(glob.glob(os.path.join(RAW_DIR, '**', '*.csv'), recursive=True))
    ok(f"Extracted {count} playlist CSVs to playlists_raw/")
    return True

# ── step 3: convert CSVs ──────────────────────────────────────────────────────
def step_convert():
    print(f"\n  {B}Converting CSVs{X}")
    hr()
    r = subprocess.run([sys.executable, os.path.join(HERE, '2_prep_csvs.py')],
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    for line in r.stdout.strip().splitlines():
        info(line)
    if r.returncode != 0:
        err("Conversion failed:"); print(r.stderr)
        return False
    return True

# ── step 4: download ──────────────────────────────────────────────────────────
def step_download(username, password):
    print(f"\n  {B}Downloading from Soulseek{X}")
    hr()

    if not os.path.exists(SOCKSEEK):
        warn("sockseek.exe not found — installing now...")
        r = subprocess.run(
            ['powershell', '-ExecutionPolicy', 'Bypass', '-File',
             os.path.join(HERE, '1_setup_sldl.ps1')],
            capture_output=True, text=True)
        if not os.path.exists(SOCKSEEK):
            err("Install failed. Run 1_setup_sldl.ps1 manually and try again.")
            return

    csvs = sorted([f for f in glob.glob(os.path.join(SLDL_DIR, '*.csv'))
                   if not os.path.basename(f).startswith('00_')])
    if not csvs:
        err("No converted CSVs found. Run option 2 first.")
        return

    os.makedirs(PLAYLISTS_DIR, exist_ok=True)
    total = len(csvs)
    print(f"  {total} playlists | destination: {D}{MUSIC_ROOT}{X}")
    print(f"  {Y}This takes hours. Ctrl+C pauses — restart to resume where you left off.{X}\n")

    for i, csv_path in enumerate(csvs, 1):
        name = os.path.splitext(os.path.basename(csv_path))[0]
        m3u  = os.path.join(PLAYLISTS_DIR, name + '.m3u')
        print(f"  {C}[{i}/{total}]{X} {B}{name}{X}")
        args = [
            SOCKSEEK, csv_path,
            '--user', username, '--pass', password,
            '--pref-format', 'flac',
            '--name-format', '{artist}\\{album}\\{title}',
            '-p', MUSIC_ROOT,
            '--skip-music-dir', MUSIC_ROOT,
            '--length-tol', '4',
            '--concurrent-searches', '2',
            '--artist-col', 'Artist', '--title-col', 'Title',
            '--album-col', 'Album',   '--length-col', 'Length',
            '--time-format', 's',
            '--write-playlist', '--playlist-path', m3u,
            '--no-progress', '--pref-strict-title',
        ]
        try:
            proc = subprocess.Popen(args, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT,
                                    text=True, encoding='utf-8', errors='replace')
            for line in proc.stdout:
                line = line.rstrip()
                if line:
                    info(line)
            proc.wait()
            ok("done") if proc.returncode == 0 else warn(f"done (exit {proc.returncode})")
        except KeyboardInterrupt:
            proc.terminate()
            print(f"\n  {Y}Paused. Run option 3 again to resume — already-downloaded tracks are skipped.{X}")
            return

    ok(f"All {total} playlists downloaded!")

# ── step 5: M3U ───────────────────────────────────────────────────────────────
def step_m3u():
    print(f"\n  {B}Generating M3U playlist files{X}")
    hr()
    r = subprocess.run([sys.executable, os.path.join(HERE, '4_generate_m3u.py')],
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    for line in r.stdout.strip().splitlines():
        info(line)
    ok(f"M3U files in: {D}{PLAYLISTS_DIR}{X}")
    if r.returncode != 0:
        warn("Some tracks unmatched — check m3u_unmatched.txt")

# ── menu ──────────────────────────────────────────────────────────────────────
def show_menu():
    cfg         = load_cfg()
    user        = cfg.get('soulseek', 'username', fallback=f'{R}not set{X}')
    sldl_ok     = os.path.exists(SOCKSEEK)
    raw_count   = len(glob.glob(os.path.join(RAW_DIR, '**', '*.csv'), recursive=True))
    conv_count  = len([f for f in glob.glob(os.path.join(SLDL_DIR, '*.csv'))
                       if not os.path.basename(f).startswith('00_')])
    m3u_count   = len(glob.glob(os.path.join(PLAYLISTS_DIR, '*.m3u')))

    banner()
    print(f"""
  {D}Soulseek  :{X}  {C}{user}{X}
  {D}Sockseek  :{X}  {(G+'installed') if sldl_ok else (Y+'not installed')}{X}
  {D}Playlists :{X}  {G}{raw_count}{X} imported  /  {G}{conv_count}{X} converted  /  {G}{m3u_count}{X} M3U files
""")
    hr()
    print(f"""
  {B}[1]{X}  Set Soulseek credentials
  {B}[2]{X}  Import playlists  {D}(opens Exportify in browser){X}
  {B}[3]{X}  Download FLACs from Soulseek
  {B}[4]{X}  Generate M3U files for Snowsky / DAP
  {D}  ─────────────────────────────────────────{X}
  {B}[5]{X}  {G}Run everything{X}  {D}(steps 1-4 in order){X}
  {B}[q]{X}  Quit
""")

# ── main ──────────────────────────────────────────────────────────────────────
def main():
    while True:
        show_menu()
        choice = input(f"  {B}>{X} ").strip().lower()

        cfg      = load_cfg()
        username = cfg.get('soulseek', 'username', fallback='')
        password = cfg.get('soulseek', 'password', fallback='')

        if choice == 'q':
            print(f"\n  {D}bye o/{X}\n")
            break

        elif choice == '1':
            username, password = step_credentials()
            pause()

        elif choice == '2':
            step_exportify()
            step_convert()
            pause()

        elif choice == '3':
            if not username:
                err("Set your Soulseek credentials first (option 1)")
            else:
                if not glob.glob(os.path.join(SLDL_DIR, '*.csv')):
                    warn("No converted CSVs yet — importing first...")
                    if not step_exportify() or not step_convert():
                        pause(); continue
                step_download(username, password)
            pause()

        elif choice == '4':
            step_m3u()
            pause()

        elif choice == '5':
            username, password = step_credentials()
            if step_exportify() and step_convert():
                step_download(username, password)
                step_m3u()
                print(f"\n  {G}{B}All done!{X}")
                print(f"  Copy your Music/ folder + Playlists/ to your Snowsky SD card.")
            pause()

        else:
            warn("Unknown option")
            time.sleep(0.8)

if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        print(f"\n\n  {D}Interrupted. bye o/{X}\n")
