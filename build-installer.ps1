#!/usr/bin/env pwsh
#
# Build Windows Installer for AW Clippy using WiX v4
#

param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "   AW Clippy Windows Installer Builder         " -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Load environment configuration (.env)
$envVersion = "0.1.0"
if (Test-Path ".env") {
    Get-Content ".env" | ForEach-Object {
        $line = $_.Trim()
        if ($line -and -not $line.StartsWith("#") -and $line.Contains("=")) {
            $parts = $line.Split("=", 2)
            $key = $parts[0].Trim()
            $val = $parts[1].Trim().Trim("'").Trim('"')
            if ($key -eq "AWCLIPPY_VERSION") {
                $envVersion = $val
            }
        }
    }
}

# Determine build version
$appVersion = if ($Version) { $Version } else { $envVersion }
Write-Host "[*] Application Version: $appVersion (from .env)" -ForegroundColor Green

# Check if WiX v4 is installed
Write-Host ""
Write-Host "[*] Checking for WiX v4..." -ForegroundColor Yellow
try {
    $wixVersion = wix --version 2>&1
    Write-Host "[OK] Found WiX: $wixVersion" -ForegroundColor Green
} catch {
    Write-Host "[X] WiX v4 not found!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please install WiX v4 with:" -ForegroundColor Yellow
    Write-Host "  dotnet tool install --global wix" -ForegroundColor Cyan
    Write-Host ""
    exit 1
}

# Ensure running instances are stopped before building/packaging
$runningProcess = Get-Process -Name "awclippy-windows" -ErrorAction SilentlyContinue
if ($runningProcess) {
    Write-Host "[*] Terminating running AWClippy instance..." -ForegroundColor Yellow
    taskkill.exe /F /IM "awclippy-windows.exe" /T 2>$null | Out-Null
    Start-Sleep -Milliseconds 500
    Write-Host "[OK] Stopped running instance" -ForegroundColor Green
}

# Step 1: Build the Rust release binary using standard release pipeline
Write-Host ""
Write-Host "[1/3] Building release binary with compiled .env configuration..." -ForegroundColor Yellow
& .\release.ps1
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] Release build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "[OK] Release binary ready" -ForegroundColor Green

# Step 2: Ensure icons directory and icon files exist
Write-Host ""
Write-Host "[2/3] Preparing icon files..." -ForegroundColor Yellow
if (-not (Test-Path "icons/icon_32x32.png") -or -not (Test-Path "icons/app_icon.ico")) {
    Write-Host "[!] Generating icon files..." -ForegroundColor Yellow
    python ../generate_icons.py
}
Write-Host "[OK] Icons ready" -ForegroundColor Green

# Step 3: Build the MSI with WiX v4
Write-Host ""
Write-Host "[3/3] Building installer package with WiX v4 (Version: $appVersion)..." -ForegroundColor Yellow

if (-not (Test-Path "installer")) {
    New-Item -ItemType Directory -Path "installer" | Out-Null
}

Push-Location installer
wix build -arch x64 -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext -d Version=$appVersion -out "AWClippy-Setup.msi" "main.wxs"
$buildResult = $LASTEXITCODE
Pop-Location

if ($buildResult -ne 0) {
    Write-Host "[X] MSI creation failed!" -ForegroundColor Red
    exit $buildResult
}

Write-Host ""
Write-Host "================================================" -ForegroundColor Green
Write-Host "   Installer created successfully!           " -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Package Version: $appVersion" -ForegroundColor Cyan
Write-Host "Output: installer\AWClippy-Setup.msi" -ForegroundColor Cyan
Write-Host ""
Write-Host "Installation Instructions:" -ForegroundColor Yellow
Write-Host "  1. Double-click installer\AWClippy-Setup.msi" -ForegroundColor White
Write-Host "  2. Follow the setup wizard (automatically replaces/upgrades previous versions)" -ForegroundColor White
Write-Host "  3. To uninstall later, click 'Uninstall AWClippy' in the Start Menu or via Installed Apps" -ForegroundColor White
Write-Host ""
