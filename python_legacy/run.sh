#!/usr/bin/env bash
# spotify-to-offline launcher for Linux / macOS
# Make executable once:  chmod +x run.sh
# Then run:  ./run.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Prefer python3, fall back to python
if command -v python3 &>/dev/null; then
    python3 run.py
elif command -v python &>/dev/null; then
    python run.py
else
    echo "Python not found. Install Python 3.8+ and try again."
    exit 1
fi
