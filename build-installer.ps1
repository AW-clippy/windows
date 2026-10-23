#!/usr/bin/env pwsh
#
# Build Windows Installer for AW Clippy using WiX v4
#

param(
    [switch]$Release = $false
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "   AW Clippy Windows Installer Builder         " -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Check if WiX v4 is installed
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

# Build the Rust application first
Write-Host ""
Write-Host "[1/3] Building release binary..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] Cargo build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "[OK] Binary built successfully" -ForegroundColor Green

# Ensure icons directory exists
Write-Host ""
Write-Host "[2/3] Preparing icon files..." -ForegroundColor Yellow
if (-not (Test-Path "icons/icon_32x32.png")) {
    Write-Host "[!] Generating icon files..." -ForegroundColor Yellow
    python ../generate_icons.py
}
Write-Host "[OK] Icons ready" -ForegroundColor Green

# Build the MSI with WiX v4
Write-Host ""
Write-Host "[3/3] Building installer package..." -ForegroundColor Yellow

# WiX v4 uses a single 'build' command with extension references
# Change to installer directory so relative paths work correctly
Push-Location installer
wix build -arch x64 -ext WixToolset.UI.wixext -out "AWClippy-Setup.msi" "main.wxs"
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
Write-Host "Output: installer\AWClippy-Setup.msi" -ForegroundColor Cyan
Write-Host ""
Write-Host "Installation Instructions:" -ForegroundColor Yellow
Write-Host "  1. Double-click installer\AWClippy-Setup.msi" -ForegroundColor White
Write-Host "  2. Follow the setup wizard" -ForegroundColor White
Write-Host "  3. Choose whether to create a desktop shortcut" -ForegroundColor White
Write-Host ""
