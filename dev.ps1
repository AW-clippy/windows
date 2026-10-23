#!/usr/bin/env pwsh
# AWClippy Windows Developer Script
# One command to: build -> run -> watch logs

param(
    [switch]$SkipBuild,
    [switch]$Help
)

if ($Help) {
    Write-Host @"
[*] AWClippy Windows Developer Script

USAGE:
  .\dev.ps1           # Build and run in debug mode
  .\dev.ps1 -SkipBuild # Skip build, just run existing binary
  .\dev.ps1 -Help     # Show this help

WORKFLOW:
  1. Builds the debug binary with cargo
  2. Runs the AWClippy Windows client
  3. Shows live logs in console

TIPS:
  • Make sure Rust toolchain is installed
  • The app will run in system tray
  • Press Ctrl+C to stop the service
  • Run again to rebuild and restart

"@
    exit 0
}

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "     AWClippy Windows Developer Mode           " -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Check Rust installation
Write-Host "[*] Checking Rust installation..." -ForegroundColor Yellow
$rustVersion = cargo --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] Rust not found! Install from https://rustup.rs/" -ForegroundColor Red
    exit 1
}
Write-Host "[OK] $rustVersion" -ForegroundColor Green
Write-Host ""

# Step 1: Build (unless skipped)
if (-not $SkipBuild) {
    Write-Host "[1/2] Building debug binary..." -ForegroundColor Yellow
    Write-Host ""
    
    $buildStart = Get-Date
    cargo build
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host ""
        Write-Host "[X] Build failed! Fix errors and try again." -ForegroundColor Red
        exit 1
    }
    
    $buildTime = ((Get-Date) - $buildStart).TotalSeconds
    Write-Host ""
    Write-Host "[OK] Build completed in $([math]::Round($buildTime, 1))s" -ForegroundColor Green
} else {
    Write-Host "[1/2] Skipping build (using existing binary)" -ForegroundColor Gray
}
Write-Host ""

# Step 2: Run the application
Write-Host "[2/2] Starting AWClippy Windows client..." -ForegroundColor Yellow
Write-Host "================================================" -ForegroundColor DarkGray
Write-Host "                  LIVE LOGS                     " -ForegroundColor Cyan
Write-Host "         (Press Ctrl+C to stop)                 " -ForegroundColor DarkGray
Write-Host "================================================" -ForegroundColor DarkGray
Write-Host ""

# Run with environment variable for debug logging
$env:RUST_LOG = "debug"

Write-Host "Starting AWClippy... (Press Ctrl+C to stop)" -ForegroundColor Cyan
Write-Host ""

try {
    # Run cargo and capture output in real-time
    cargo run
}
catch {
    Write-Host ""
    Write-Host "[*] Service stopped" -ForegroundColor Yellow
}
