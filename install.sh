#!/usr/bin/env bash
# spotify-to-offline installer for Linux and macOS
#
# Run this in your terminal:
#   curl -fsSL https://raw.githubusercontent.com/kadokonkwo/spotify-to-offline/main/install.sh | bash
#
# What it does:
#   1. Detects your OS and CPU architecture
#   2. Downloads the matching binary from the latest GitHub Release
#   3. Runs `s2o install` which copies it to ~/.local/bin/ and updates your shell profiles

set -euo pipefail

REPO="kadokonkwo/spotify-to-offline"

echo ""
echo "  spotify-to-offline installer"
echo "  github.com/$REPO"
echo ""

# ── Detect platform ───────────────────────────────────────────────────────────

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  OS_LABEL="linux"  ;;
  darwin) OS_LABEL="macos"  ;;
  *)
    echo "  Error: unsupported OS '$OS'."
    echo "  On Windows, use the PowerShell installer instead:"
    echo "    irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64)        ARCH_LABEL="x64"   ;;
  aarch64|arm64) ARCH_LABEL="arm64" ;;
  *)
    echo "  Error: unsupported CPU architecture '$ARCH'."
    exit 1
    ;;
esac

ARTIFACT="s2o-${OS_LABEL}-${ARCH_LABEL}"
echo "  Platform: ${OS_LABEL}/${ARCH_LABEL}  →  ${ARTIFACT}"

# ── Fetch latest release ──────────────────────────────────────────────────────

echo "  Fetching latest release..."

API_RESPONSE=$(curl -sf "https://api.github.com/repos/${REPO}/releases/latest" || true)
if [[ -z "$API_RESPONSE" ]]; then
  echo "  Error: could not reach GitHub API. Check your internet connection."
  exit 1
fi

DOWNLOAD_URL=$(echo "$API_RESPONSE" \
  | grep '"browser_download_url"' \
  | grep "\"${ARTIFACT}\"" \
  | head -1 \
  | cut -d'"' -f4)

if [[ -z "$DOWNLOAD_URL" ]]; then
  echo "  Error: could not find '${ARTIFACT}' in the latest release."
  echo "  Check https://github.com/${REPO}/releases for available downloads."
  exit 1
fi

TAG=$(echo "$API_RESPONSE" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
echo "  Found:  s2o ${TAG}"

# ── Download ──────────────────────────────────────────────────────────────────

TMP=$(mktemp)
# shellcheck disable=SC2064
trap "rm -f '$TMP'" EXIT

echo "  Downloading..."
curl -sfL "$DOWNLOAD_URL" -o "$TMP"
chmod +x "$TMP"

# ── Self-install ──────────────────────────────────────────────────────────────
# s2o install copies the binary to ~/.local/bin/s2o and patches shell profiles.

echo "  Installing..."
"$TMP" install

echo ""
echo "  Done! Open a new terminal and run:"
echo "    s2o"
echo ""
