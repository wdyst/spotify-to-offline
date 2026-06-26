"""
spotify-to-offline -- interactive launcher
  Windows : double-click run.bat   OR   python run.py
  Linux   : ./run.sh               OR   python3 run.py
  macOS   : ./run.sh               OR   python3 run.py
"""
import os, sys, re, csv, glob, time, platform, shutil
import subprocess, configparser, webbrowser, zipfile
sys.stdout.reconfigure(encoding='utf-8')
if os.name == 'nt':
    os.system('')  # enable ANSI on Windows

C,G,Y,R,B,D,X = '\033[96m','\033[92m','\033[93m','\033[91m','\033[1m','\033[2m','\033[0m'

HERE     = os.path.dirname(os.path.abspath(__file__))
CONFIG   = os.path.join(HERE, 'config.ini')
IS_WIN   = platform.system() == 'Windows'
CLRCMD   = 'cls' if IS_WIN else 'clear'
EXE_NAME = 'sockseek.exe' if IS_WIN else 'sockseek'
RAW_DIR  = os.path.join(HERE, 'playlists_raw')
SLDL_DIR = os.path.join(HERE, 'playlists_sldl')

PROVIDERS = ['soulseek', 'ytdlp', 'custom']

# ── helpers ───────────────────────────────────────────────────────────────────
def ok(m):   print(f"  {G}✓{X}  {m}")
def warn(m): print(f"  {Y}!{X}  {m}")
def err(m):  print(f"  {R}✗{X}  {m}")
def info(m): print(f"     {D}{m}{X}")
def hr():    print(f"  {D}{'─'*46}{X}")
def pause(): input(f"\n  {D}Press Enter to return to menu...{X}")

def banner():
    os.system(CLRCMD)
    print(f"""
{C}{B}  +------------------------------------------------+
  |           spotify-to-offline  v2.0             |
  |       Spotify -> FLAC -> Snowsky / DAP         |
  +------------------------------------------------+{X}""")

# ── config ────────────────────────────────────────────────────────────────────
def load_cfg():
    c = configparser.RawConfigParser()
    if os.path.exists(CONFIG):
        c.read(CONFIG, encoding='utf-8')
    return c

def save_cfg(c):
    with open(CONFIG, 'w', encoding='utf-8') as f:
        c.write(f)

def cfg_get(c, section, key, fallback=''):
    return c.get(section, key, fallback=fallback)

def cfg_set(c, section, key, value):
    if not c.has_section(section):
        c.add_section(section)
    c.set(section, key, value)

def get_music_root(cfg):
    v = cfg_get(cfg, 'paths', 'music_root')
    return v if v else os.path.join(os.path.expanduser('~'), 'Music')

def get_playlists_dir(cfg):
    v = cfg_get(cfg, 'paths', 'playlists_dir')
    return v if v else os.path.join(get_music_root(cfg), 'Playlists')

def get_sockseek(cfg):
    v = cfg_get(cfg, 'paths', 'sockseek_path')
    return v if v else os.path.join(HERE, EXE_NAME)

def get_provider(cfg):
    return cfg_get(cfg, 'provider', 'type') or 'soulseek'

def get_ytdlp(cfg):
    v = cfg_get(cfg, 'provider', 'ytdlp_path')
    return v if v else 'yt-dlp'

def get_custom_cmd(cfg):
    return cfg_get(cfg, 'provider', 'custom_cmd')

# ── step 1: credentials ───────────────────────────────────────────────────────
def step_credentials():
    cfg = load_cfg()
    cur_user = cfg_get(cfg, 'soulseek', 'username')
    cur_pass = cfg_get(cfg, 'soulseek', 'password')
    print(f"\n  {B}Soulseek credentials{X}  {D}(free account at slsknet.org){X}")
    hr()
    if cur_user and cur_pass:
        ok(f"Already saved: {C}{cur_user}{X}")
        if input("  Change credentials? [y/N] ").strip().lower() != 'y':
            return cur_user, cur_pass
    username = input("  Username: ").strip()
    password = input("  Password: ").strip()
    if username and password:
        cfg_set(cfg, 'soulseek', 'username', username)
        cfg_set(cfg, 'soulseek', 'password', password)
        save_cfg(cfg)
        ok("Saved!")
    else:
        warn("Skipped — nothing saved.")
    pause()
    return username, password

# ── step 2: install sockseek ──────────────────────────────────────────────────
def step_setup():
    print(f"\n  {B}Install / update Sockseek{X}")
    hr()
    setup = os.path.join(HERE, '1_setup.py')
    if not os.path.exists(setup):
        err("1_setup.py not found in script directory.")
        pause(); return
    r = subprocess.run([sys.executable, setup], text=True, encoding='utf-8', errors='replace')
    if r.returncode == 0:
        ok("Sockseek installed!")
    else:
        warn(f"Setup finished with exit code {r.returncode} — check output above.")
    pause()

# ── step 3: import playlists ──────────────────────────────────────────────────
def step_import():
    print(f"\n  {B}Import playlists from Exportify{X}")
    hr()
    print("  Opening exportify.net in your browser...")
    webbrowser.open("https://exportify.net")
    print(f"  {D}1. Sign in with Spotify")
    print(f"  2. Click 'Export All' → save the ZIP{X}")
    print()
    zip_path = input("  Drag the ZIP here (or paste path): ").strip().strip('"').strip("'")
    if not zip_path or not os.path.exists(zip_path):
        err(f"Not found: {zip_path}"); pause(); return
    os.makedirs(RAW_DIR, exist_ok=True)
    with zipfile.ZipFile(zip_path, 'r') as z:
        count = len(z.namelist())
        z.extractall(RAW_DIR)
    ok(f"Extracted {count} files to playlists_raw/")
    info("Converting CSVs for Sockseek...")
    r = subprocess.run([sys.executable, os.path.join(HERE, '2_prep_csvs.py')],
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    for line in r.stdout.strip().splitlines(): info(line)
    conv = len([f for f in glob.glob(os.path.join(SLDL_DIR, '*.csv'))
                if not os.path.basename(f).startswith('00_')])
    ok(f"{conv} playlists ready for download") if r.returncode == 0 else warn("Check output above.")
    pause()

# ── settings ──────────────────────────────────────────────────────────────────
def step_settings():
    while True:
        cfg = load_cfg()
        prov = get_provider(cfg)
        os.system(CLRCMD)
        banner()
        print(f"\n  {B}Settings{X}\n")
        hr()
        print(f"  {B}[1]{X}  Music root     {D}:{X} {C}{get_music_root(cfg)}{X}")
        print(f"  {B}[2]{X}  Playlists dir  {D}:{X} {C}{get_playlists_dir(cfg)}{X}")
        print(f"  {B}[3]{X}  Provider       {D}:{X} {C}{prov}{X}  {D}(soulseek / ytdlp / custom){X}")
        print(f"  {B}[4]{X}  Sockseek path  {D}:{X} {C}{get_sockseek(cfg)}{X}")
        print(f"  {B}[5]{X}  yt-dlp path    {D}:{X} {C}{get_ytdlp(cfg)}{X}")
        if prov == 'custom':
            print(f"  {B}[6]{X}  Custom command {D}:{X} {C}{get_custom_cmd(cfg) or '(not set)'}{X}")
            print(f"       {D}Placeholders: {{artist}} {{title}} {{album}} {{output}}{X}")
        hr()
        print(f"  {B}[b]{X}  Back to main menu\n")

        ch = input("  Choice: ").strip().lower()
        if ch == 'b': return

        def _prompt(label, current):
            v = input(f"  {label} [{current}]: ").strip()
            return v or None

        if ch == '1':
            v = _prompt("Music root", get_music_root(cfg))
            if v: cfg_set(cfg, 'paths', 'music_root', v); save_cfg(cfg); ok(f"Set: {v}")
        elif ch == '2':
            v = _prompt("Playlists dir", get_playlists_dir(cfg))
            if v: cfg_set(cfg, 'paths', 'playlists_dir', v); save_cfg(cfg); ok(f"Set: {v}")
        elif ch == '3':
            print(f"  Options: {', '.join(PROVIDERS)}")
            v = input("  Provider: ").strip().lower()
            if v in PROVIDERS:
                cfg_set(cfg, 'provider', 'type', v); save_cfg(cfg); ok(f"Provider: {v}")
            else:
                warn(f"Unknown provider — choose from: {', '.join(PROVIDERS)}")
        elif ch == '4':
            v = _prompt("Sockseek path", get_sockseek(cfg))
            if v: cfg_set(cfg, 'paths', 'sockseek_path', v); save_cfg(cfg); ok(f"Set: {v}")
        elif ch == '5':
            v = _prompt("yt-dlp path", get_ytdlp(cfg))
            if v: cfg_set(cfg, 'provider', 'ytdlp_path', v); save_cfg(cfg); ok(f"Set: {v}")
        elif ch == '6' and prov == 'custom':
            print(f"  Example: yt-dlp \"ytsearch1:{{artist}} {{title}}\" -o {{output}}/{{artist}}/{{title}}.%(ext)s")
            v = input("  Command: ").strip()
            if v: cfg_set(cfg, 'provider', 'custom_cmd', v); save_cfg(cfg); ok("Command saved.")
        time.sleep(0.4)

# ── download providers ────────────────────────────────────────────────────────
def _run_proc(args):
    """Stream a subprocess, return exit code."""
    try:
        proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, encoding='utf-8', errors='replace')
        for line in proc.stdout:
            line = line.rstrip()
            if line: info(line)
        proc.wait()
        return proc.returncode
    except FileNotFoundError:
        err(f"Command not found: {args[0]}")
        return 1

def download_soulseek(cfg, csv_path, m3u_path):
    sockseek = get_sockseek(cfg)
    if not os.path.exists(sockseek):
        err(f"Sockseek not found: {sockseek}")
        warn("Run option 2 (Install Sockseek) or set path in Settings."); return False
    username = cfg_get(cfg, 'soulseek', 'username')
    password = cfg_get(cfg, 'soulseek', 'password')
    if not username or not password:
        err("No credentials — use option 1 first."); return False
    sep = os.sep
    args = [
        sockseek, csv_path,
        '--user', username, '--pass', password,
        '--pref-format', 'flac',
        '--name-format', f'{{artist}}{sep}{{album}}{sep}{{title}}',
        '-p', get_music_root(cfg),
        '--skip-music-dir', get_music_root(cfg),
        '--length-tol', '4', '--concurrent-searches', '2',
        '--artist-col', 'Artist', '--title-col', 'Title',
        '--album-col', 'Album',   '--length-col', 'Length',
        '--time-format', 's',
        '--write-playlist', '--playlist-path', m3u_path,
        '--no-progress', '--pref-strict-title',
    ]
    return _run_proc(args) == 0

def download_ytdlp(cfg, csv_path, m3u_path):
    ytdlp = get_ytdlp(cfg)
    if not shutil.which(ytdlp):
        err(f"yt-dlp not found: {ytdlp}")
        warn("Install: pip install yt-dlp  or  https://github.com/yt-dlp/yt-dlp"); return False
    music_root = get_music_root(cfg)
    tracks = []
    with open(csv_path, encoding='utf-8') as f:
        tracks = list(csv.DictReader(f))
    ok_count = 0
    for i, t in enumerate(tracks, 1):
        artist, title, album = t.get('Artist','').strip(), t.get('Title','').strip(), t.get('Album','').strip()
        out_dir = os.path.join(music_root, artist, album)
        os.makedirs(out_dir, exist_ok=True)
        out_tpl = os.path.join(out_dir, f"{title}.%(ext)s")
        info(f"[{i}/{len(tracks)}] {artist} - {title}")
        args = [ytdlp, f"ytsearch1:{artist} - {title}", '-x', '--audio-format', 'flac',
                '--audio-quality', '0', '-o', out_tpl, '--no-playlist', '-q', '--progress',
                '--match-filter', '!is_live']
        if subprocess.run(args).returncode == 0: ok_count += 1
    ok(f"Downloaded {ok_count}/{len(tracks)} tracks via yt-dlp")
    return True

def download_custom(cfg, csv_path, m3u_path):
    template = get_custom_cmd(cfg)
    if not template:
        err("No custom command set — go to Settings → [6] Custom command."); return False
    music_root = get_music_root(cfg)
    tracks = []
    with open(csv_path, encoding='utf-8') as f:
        tracks = list(csv.DictReader(f))
    for i, t in enumerate(tracks, 1):
        artist, title, album = t.get('Artist','').strip(), t.get('Title','').strip(), t.get('Album','').strip()
        cmd = template.format(artist=artist, title=title, album=album, output=music_root)
        info(f"[{i}/{len(tracks)}] {cmd}")
        subprocess.run(cmd, shell=True)
    return True

# ── step 4: download ──────────────────────────────────────────────────────────
def step_download():
    cfg  = load_cfg()
    prov = get_provider(cfg)
    print(f"\n  {B}Download via{X} {C}{prov}{X}")
    hr()
    csvs = sorted([f for f in glob.glob(os.path.join(SLDL_DIR, '*.csv'))
                   if not os.path.basename(f).startswith('00_')])
    if not csvs:
        err("No converted playlists found. Run option 3 (Import) first.")
        pause(); return
    playlists_dir = get_playlists_dir(cfg)
    music_root    = get_music_root(cfg)
    os.makedirs(playlists_dir, exist_ok=True)
    total = len(csvs)
    print(f"  {total} playlists | destination: {D}{music_root}{X}")
    if prov == 'soulseek':
        print(f"  {R}⚠  VPN recommended:{X} Soulseek exposes your IP to peers.")
        print(f"     {D}Mullvad / ProtonVPN work well with P2P. Already on one? Carry on.{X}")
    print(f"  {Y}This may take hours. Ctrl+C pauses — restart to resume.{X}\n")
    dispatch = {'soulseek': download_soulseek, 'ytdlp': download_ytdlp, 'custom': download_custom}
    fn = dispatch.get(prov, download_soulseek)
    for i, csv_path in enumerate(csvs, 1):
        name = os.path.splitext(os.path.basename(csv_path))[0]
        m3u  = os.path.join(playlists_dir, name + '.m3u')
        print(f"  {C}[{i}/{total}]{X} {B}{name}{X}")
        try:
            ok("done") if fn(cfg, csv_path, m3u) else warn("done (some errors)")
        except KeyboardInterrupt:
            print(f"\n  {Y}Paused. Run option 4 again to resume — already-downloaded tracks are skipped.{X}")
            return
    ok(f"All {total} playlists done!")
    pause()

# ── step 5: M3U ───────────────────────────────────────────────────────────────
def step_m3u():
    cfg = load_cfg()
    music_root    = get_music_root(cfg)
    playlists_dir = get_playlists_dir(cfg)
    print(f"\n  {B}Generating M3U playlist files{X}")
    hr()
    env = os.environ.copy()
    env['MUSIC_ROOT']    = music_root
    env['PLAYLISTS_DIR'] = playlists_dir
    r = subprocess.run([sys.executable, os.path.join(HERE, '4_generate_m3u.py')],
                       capture_output=True, text=True, encoding='utf-8', errors='replace', env=env)
    for line in r.stdout.strip().splitlines(): info(line)
    if r.returncode == 0:
        ok(f"M3U files written to: {D}{playlists_dir}{X}")
    else:
        warn("Some tracks unmatched — check m3u_unmatched.txt")
    pause()

# ── full run ──────────────────────────────────────────────────────────────────
def step_full_run():
    cfg = load_cfg()
    if not cfg_get(cfg, 'soulseek', 'username') and get_provider(cfg) == 'soulseek':
        step_credentials()
    if not os.path.exists(get_sockseek(cfg)) and get_provider(cfg) == 'soulseek':
        step_setup()
    step_import()
    step_download()
    step_m3u()

# ── menu ──────────────────────────────────────────────────────────────────────
def show_menu():
    cfg      = load_cfg()
    prov     = get_provider(cfg)
    user     = cfg_get(cfg, 'soulseek', 'username') or f'{R}not set{X}'
    sldl_ok  = os.path.exists(get_sockseek(cfg))
    raw_c    = len(glob.glob(os.path.join(RAW_DIR,  '**', '*.csv'), recursive=True))
    conv_c   = len([f for f in glob.glob(os.path.join(SLDL_DIR, '*.csv'))
                    if not os.path.basename(f).startswith('00_')])
    m3u_c    = len(glob.glob(os.path.join(get_playlists_dir(cfg), '*.m3u')))
    banner()
    print(f"""
  {D}Provider  :{X}  {C}{prov}{X}
  {D}Soulseek  :{X}  {C}{user}{X}
  {D}Sockseek  :{X}  {(G+'installed') if sldl_ok else (Y+'not installed')}{X}
  {D}Playlists :{X}  {G}{raw_c}{X} imported  /  {G}{conv_c}{X} converted  /  {G}{m3u_c}{X} M3U files
""")
    hr()
    print(f"""
  {B}[1]{X}  Set Soulseek credentials
  {B}[2]{X}  Install Sockseek
  {B}[3]{X}  Import playlists  {D}(opens Exportify in browser){X}
  {B}[4]{X}  Download  {D}(via {prov}){X}
  {B}[5]{X}  Generate M3U files for Snowsky / DAP
  {B}[6]{X}  Full run  {D}(steps 3 → 4 → 5){X}
  {D}{'─'*46}{X}
  {B}[s]{X}  Settings  {D}(paths, provider, custom commands){X}
  {B}[q]{X}  Quit
""")
    hr()

def main():
    while True:
        show_menu()
        ch = input(f"  Choice: ").strip().lower()
        if   ch == '1': step_credentials()
        elif ch == '2': step_setup()
        elif ch == '3': step_import()
        elif ch == '4': step_download()
        elif ch == '5': step_m3u()
        elif ch == '6': step_full_run()
        elif ch == 's': step_settings()
        elif ch == 'q': print(f"\n  {G}Bye!{X}\n"); break

if __name__ == '__main__':
    main()
