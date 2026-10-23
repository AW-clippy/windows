#!/usr/bin/env pwsh
#
# Build Windows Installer for AW Clippy
# Prerequisites: Install WiX Toolset from https://wixtoolset.org/
#

param(
    [switch]$Release = $false
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "   AW Clippy Windows Installer Builder         " -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Check if WiX is installed
Write-Host "[*] Checking for WiX Toolset..." -ForegroundColor Yellow
$wixPath = $null
$possiblePaths = @(
    "${env:ProgramFiles(x86)}\WiX Toolset v3.11\bin",
    "${env:ProgramFiles}\WiX Toolset v3.11\bin",
    "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin",
    "${env:ProgramFiles}\WiX Toolset v3.14\bin"
)

foreach ($path in $possiblePaths) {
    if (Test-Path "$path\candle.exe") {
        $wixPath = $path
        break
    }
}

if (-not $wixPath) {
    Write-Host "[X] WiX Toolset not found!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please install WiX Toolset from:" -ForegroundColor Yellow
    Write-Host "  https://wixtoolset.org/releases/" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Recommended: WiX Toolset v3.11 or newer" -ForegroundColor Yellow
    exit 1
}

Write-Host "[OK] Found WiX at: $wixPath" -ForegroundColor Green

# Build the Rust application first
Write-Host ""
Write-Host "[1/4] Building release binary..." -ForegroundColor Yellow
$buildProfile = if ($Release) { "--release" } else { "--release" }
cargo build $buildProfile
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] Cargo build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "[OK] Binary built successfully" -ForegroundColor Green

# Ensure icons directory exists
Write-Host ""
Write-Host "[2/4] Preparing icon files..." -ForegroundColor Yellow
if (-not (Test-Path "icons/icon_32x32.png")) {
    Write-Host "[!] Generating icon files..." -ForegroundColor Yellow
    python ../generate_icons.py
}
Write-Host "[OK] Icons ready" -ForegroundColor Green

# Compile WiX source
Write-Host ""
Write-Host "[3/4] Compiling installer..." -ForegroundColor Yellow
$candleExe = Join-Path $wixPath "candle.exe"
$lightExe = Join-Path $wixPath "light.exe"

& $candleExe -arch x64 -out "installer\awclippy.wixobj" "installer\main.wxs"
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] WiX compilation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "[OK] WiX object created" -ForegroundColor Green

# Link and create MSI
Write-Host ""
Write-Host "[4/4] Creating MSI package..." -ForegroundColor Yellow
& $lightExe -ext WixUIExtension -out "installer\AWClippy-Setup.msi" "installer\awclippy.wixobj"
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] MSI creation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "================================================" -ForegroundColor Green
Write-Host "   ✓ Installer created successfully!           " -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Output: " -NoNewline
Write-Host "installer\AWClippy-Setup.msi" -ForegroundColor Cyan
Write-Host ""
Write-Host "To install:" -ForegroundColor Yellow
Write-Host "  1. Run installer\AWClippy-Setup.msi" -ForegroundColor White
Write-Host "  2. Follow the installation wizard" -ForegroundColor White
Write-Host ""
Write-Host "To uninstall:" -ForegroundColor Yellow
Write-Host "  1. Open Settings > Apps > Installed apps" -ForegroundColor White
Write-Host "  2. Find 'AW Clippy' and click Uninstall" -ForegroundColor White
Write-Host ""
