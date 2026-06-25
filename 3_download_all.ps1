# =============================================================================
# 3_download_all.ps1
# Batch-download all playlists via Sockseek (slsk-batchdl).
# EDIT YOUR CREDENTIALS BELOW, then run:
#   powershell -ExecutionPolicy Bypass -File "3_download_all.ps1"
#
# Safe to stop and restart — already-downloaded tracks are skipped.
# Tip: run in a separate PowerShell window; this takes hours for large libraries.
# =============================================================================

# ============================================================
#   YOUR SOULSEEK CREDENTIALS  (fill these in)
# ============================================================
$SlskUser = "YOUR_SOULSEEK_USERNAME"
$SlskPass = "YOUR_SOULSEEK_PASSWORD"
# ============================================================

$ToolDir      = $PSScriptRoot                        # folder containing this script
$SldlExe      = "$ToolDir\sockseek.exe"              # renamed from sldl in v3+
$CsvDir       = "$ToolDir\playlists_sldl"
$MusicRoot    = "$env:USERPROFILE\Music"             # change this if needed
$PlaylistsDir = "$MusicRoot\Playlists"
$LogFile      = "$ToolDir\download_log.txt"
New-Item -ItemType Directory -Force -Path $PlaylistsDir | Out-Null

# Validate setup
if (-not (Test-Path -LiteralPath $SldlExe)) {
    Write-Host "ERROR: sockseek.exe not found. Run 1_setup_sldl.ps1 first." -ForegroundColor Red
    exit 1
}
if ($SlskUser -eq "YOUR_SOULSEEK_USERNAME") {
    Write-Host "ERROR: Edit this script and fill in your Soulseek username/password." -ForegroundColor Red
    exit 1
}
$csvFiles = Get-ChildItem -Path $CsvDir -Filter "*.csv" |
             Where-Object { $_.Name -notmatch "^00_all_tracks" } |
             Sort-Object Name

if (-not $csvFiles) {
    Write-Host "ERROR: No playlist CSVs found in $CsvDir. Run 2_prep_csvs.py first." -ForegroundColor Red
    exit 1
}

Write-Host "Starting download of $($csvFiles.Count) playlists to $MusicRoot" -ForegroundColor Cyan
Write-Host "Log: $LogFile"
Write-Host ""

Add-Content -Path $LogFile -Value "=== Download session started $(Get-Date) ==="

$total   = $csvFiles.Count
$current = 0

foreach ($csv in $csvFiles) {
    $current++
    $playlistName = [System.IO.Path]::GetFileNameWithoutExtension($csv.Name)
    Write-Host "[$current/$total] $playlistName" -ForegroundColor Yellow

    # sockseek v3 flags:
    #   --pref-format flac        : prefer FLAC; fall back to mp3/m4a if unavailable
    #   --name-format             : {artist}\{album}\{title} structure under MusicRoot
    #   --skip-music-dir          : skip if track already exists in MusicRoot (scans library)
    #   --length-tol 4            : allow 4-second duration mismatch (covers intros/outros)
    #   --concurrent-searches 2   : parallel searches (polite to network)
    #   --write-playlist          : create M3U in Playlists\ for this playlist
    #   --no-progress             : cleaner log when running unattended
    #   --artist-col/title-col/etc: match column names in our converted CSVs
    #   --time-format s           : Length column is in seconds
    $PlaylistM3U = "$PlaylistsDir\$playlistName.m3u"
    $sArgs = @(
        $csv.FullName,
        "--user",            $SlskUser,
        "--pass",            $SlskPass,
        "--pref-format",     "flac",
        "--name-format",     "{artist}\{album}\{title}",
        "-p",                $MusicRoot,
        "--skip-music-dir",  $MusicRoot,
        "--length-tol",      "4",
        "--concurrent-searches","2",
        "--artist-col",      "Artist",
        "--title-col",       "Title",
        "--album-col",       "Album",
        "--length-col",      "Length",
        "--time-format",     "s",
        "--write-playlist",
        "--playlist-path",   $PlaylistM3U,
        "--no-progress",
        "--pref-strict-title"
    )

    $result = & $SldlExe @sArgs 2>&1
    $exitCode = $LASTEXITCODE

    $logLine = "[$(Get-Date -Format 'HH:mm:ss')] $playlistName (exit $exitCode)"
    Add-Content -Path $LogFile -Value $logLine
    if ($exitCode -ne 0) {
        Write-Host "  WARNING: exit code $exitCode" -ForegroundColor DarkYellow
        Add-Content -Path $LogFile -Value "  STDERR: $result"
    } else {
        Write-Host "  Done." -ForegroundColor Green
    }

    # Brief pause between playlists to be polite
    Start-Sleep -Seconds 3
}

Write-Host ""
Write-Host "All playlists processed." -ForegroundColor Green
Write-Host "Now run: python 4_generate_m3u.py"
Add-Content -Path $LogFile -Value "=== Session ended $(Get-Date) ==="
