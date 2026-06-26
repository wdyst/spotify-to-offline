# spotify-to-offline installer for Windows
#
# Run this in PowerShell:
#   irm https://raw.githubusercontent.com/kadokonkwo/spotify-to-offline/main/install.ps1 | iex
#
# What it does:
#   1. Downloads the latest s2o-windows-x64.exe from GitHub Releases
#   2. Runs `s2o install` which copies it to %LOCALAPPDATA%\s2o\bin\ and adds that to your PATH
#   3. Cleans up the temp file

$ErrorActionPreference = 'Stop'
$repo = 'kadokonkwo/spotify-to-offline'

Write-Host ""
Write-Host "  spotify-to-offline installer" -ForegroundColor Cyan
Write-Host "  github.com/$repo" -ForegroundColor DarkGray
Write-Host ""

# ── Fetch latest release info ─────────────────────────────────────────────────

Write-Host "  Fetching latest release..." -ForegroundColor DarkGray
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
} catch {
    Write-Host "  Error: could not reach GitHub API. Check your internet connection." -ForegroundColor Red
    exit 1
}

$asset = $release.assets | Where-Object { $_.name -eq 's2o-windows-x64.exe' }
if (-not $asset) {
    Write-Host "  Error: 's2o-windows-x64.exe' not found in release $($release.tag_name)." -ForegroundColor Red
    Write-Host "  Check https://github.com/$repo/releases for available downloads." -ForegroundColor DarkGray
    exit 1
}

Write-Host "  Found:  s2o $($release.tag_name)" -ForegroundColor DarkGray

# ── Download ──────────────────────────────────────────────────────────────────

$tmp = Join-Path $env:TEMP 's2o-install.exe'
Write-Host "  Downloading..." -ForegroundColor DarkGray
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp -UseBasicParsing
} catch {
    Write-Host "  Error: download failed. $_" -ForegroundColor Red
    exit 1
}

# ── Self-install ──────────────────────────────────────────────────────────────
# s2o install copies the binary to %LOCALAPPDATA%\s2o\bin\s2o.exe
# and adds that directory to the user's PATH via the Windows registry.

Write-Host "  Installing..." -ForegroundColor DarkGray
& $tmp install
$exit_code = $LASTEXITCODE

Remove-Item $tmp -ErrorAction SilentlyContinue

if ($exit_code -ne 0) {
    Write-Host ""
    Write-Host "  Installation failed (exit $exit_code)." -ForegroundColor Red
    exit $exit_code
}

Write-Host ""
Write-Host "  Done! Open a new terminal and run:" -ForegroundColor Green
Write-Host "    s2o" -ForegroundColor White
Write-Host ""
