# =============================================================================
# 1_setup_sldl.ps1
# Download and install Sockseek (formerly slsk-batchdl / sldl) — run once.
# No administrator privileges required.
# https://github.com/fiso64/slsk-batchdl
# =============================================================================

$toolDir    = "C:\Users\kado\Music\spotify_tools"
$sldlExe    = "$toolDir\sockseek.exe"   # tool was renamed to sockseek in v3+

if (Test-Path -LiteralPath $sldlExe) {
    Write-Host "sockseek already installed at $sldlExe" -ForegroundColor Green
    & $sldlExe --version
    exit 0
}

Write-Host "Fetching latest sldl release info..." -ForegroundColor Cyan
try {
    $release = Invoke-RestMethod "https://api.github.com/repos/fiso64/slsk-batchdl/releases/latest"
    $asset   = $release.assets | Where-Object { $_.name -like "*win-x64*.zip" } | Select-Object -First 1

    if (-not $asset) {
        Write-Host "Could not find win-x64 asset. Available assets:" -ForegroundColor Yellow
        $release.assets | ForEach-Object { Write-Host "  $($_.name)" }
        exit 1
    }

    Write-Host "Downloading $($asset.name) ..." -ForegroundColor Cyan
    $zipPath = "$env:TEMP\sldl_download.zip"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing

    Write-Host "Extracting..."
    Expand-Archive -Path $zipPath -DestinationPath "$env:TEMP\sldl_extract" -Force

    # Find the exe — named sockseek.exe in v3+, sldl.exe in older versions
    $foundExe = Get-ChildItem -Path "$env:TEMP\sldl_extract" -Recurse -Include "sockseek.exe","sldl.exe" | Select-Object -First 1
    if ($foundExe) {
        Copy-Item -LiteralPath $foundExe.FullName -Destination $sldlExe -Force
        Write-Host "sockseek installed: $sldlExe" -ForegroundColor Green
        & $sldlExe --version
    } else {
        Write-Host "ERROR: sldl.exe not found in archive." -ForegroundColor Red
        exit 1
    }
    Remove-Item $zipPath -Force -ErrorAction SilentlyContinue
    Remove-Item "$env:TEMP\sldl_extract" -Recurse -Force -ErrorAction SilentlyContinue
}
catch {
    Write-Host "ERROR: $_" -ForegroundColor Red
    Write-Host ""
    Write-Host "Manual install: go to https://github.com/fiso64/slsk-batchdl/releases"
    Write-Host "Download the win-x64 .zip, extract sldl.exe to: $toolDir"
    exit 1
}
