#!/usr/bin/env python3
"""
Cross-platform Sockseek installer.
Downloads the correct binary for Windows, Linux, or macOS.
Run directly:  python 1_setup.py
Or via menu:   option 2 in run.py
"""
import os, sys, platform, urllib.request, zipfile, stat, shutil

HERE    = os.path.dirname(os.path.abspath(__file__))
VERSION = "3.0.1"
BASE    = f"https://github.com/fiso64/slsk-batchdl/releases/download/v{VERSION}"

system = platform.system()
machine = platform.machine().lower()

if system == "Windows":
    asset  = "sockseek_win-x64.zip"
    binary = "sockseek.exe"
elif system == "Linux":
    asset  = "sockseek_linux-x64.zip"
    binary = "sockseek"
elif system == "Darwin":
    asset  = "sockseek_osx-x64.zip"
    binary = "sockseek"
else:
    print(f"Unsupported OS: {system}")
    sys.exit(1)

dest_bin = os.path.join(HERE, binary)

# Skip if already installed and up to date
if os.path.exists(dest_bin):
    ans = input(f"Sockseek already installed at {dest_bin}. Re-download? [y/N] ").strip().lower()
    if ans != 'y':
        print("Skipped.")
        sys.exit(0)

zip_path = os.path.join(HERE, asset)
url      = f"{BASE}/{asset}"

print(f"Downloading sockseek v{VERSION} for {system} ({machine})...")
print(f"  {url}")

try:
    urllib.request.urlretrieve(url, zip_path)
except Exception as e:
    print(f"Download failed: {e}")
    print("Check your internet connection or download manually from:")
    print(f"  https://github.com/fiso64/slsk-batchdl/releases/tag/v{VERSION}")
    sys.exit(1)

print("Extracting...")
with zipfile.ZipFile(zip_path, 'r') as z:
    # Extract only the binary (avoid overwriting other files)
    for member in z.namelist():
        if os.path.basename(member) == binary:
            z.extract(member, HERE)
            extracted = os.path.join(HERE, member)
            if extracted != dest_bin:
                shutil.move(extracted, dest_bin)
            break
    else:
        # Fallback: extract everything
        z.extractall(HERE)

os.remove(zip_path)

# Make executable on non-Windows
if system != "Windows":
    current = os.stat(dest_bin).st_mode
    os.chmod(dest_bin, current | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

print(f"Done! Sockseek installed: {dest_bin}")
