#!/usr/bin/env pwsh
# AWClippy Windows Release Build Script
# Build optimized production binary

param(
    [switch]$Install,
    [switch]$Help
)

if ($Help) {
    Write-Host @"
[*] AWClippy Windows Release Build Script

USAGE:
  .\release.ps1           # Build optimized release binary
  .\release.ps1 -Install  # Build and copy to Program Files
  .\release.ps1 -Help     # Show this help

WORKFLOW:
  1. Builds optimized release binary with cargo
  2. Strips debug symbols for minimal size
  3. Optionally installs to system location

OUTPUT:
  • Binary: target\release\awclippy-windows.exe
  • Size: ~2-4 MB (optimized)
  • Ready for distribution

"@
    exit 0
}

$ErrorActionPreference = "Stop"

Write-Host "========================================================" -ForegroundColor Magenta
Write-Host "   Building AWClippy Windows RELEASE                    " -ForegroundColor Magenta
Write-Host "========================================================" -ForegroundColor Magenta
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

# Stop any running instances to avoid "Access denied" errors
Write-Host "[*] Checking for running instances..." -ForegroundColor Yellow
$runningProcess = Get-Process -Name "awclippy-windows" -ErrorAction SilentlyContinue
if ($runningProcess) {
    Write-Host "[*] Stopping running AWClippy instance..." -ForegroundColor Yellow
    Stop-Process -Name "awclippy-windows" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
    Write-Host "[OK] Stopped running instance" -ForegroundColor Green
} else {
    Write-Host "[OK] No running instances found" -ForegroundColor Green
}
Write-Host ""

# Build release binary
Write-Host "[*] Building optimized release binary..." -ForegroundColor Yellow
Write-Host ""

$buildStart = Get-Date
cargo build --release

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "[X] Release build failed!" -ForegroundColor Red
    exit 1
}

$buildTime = ((Get-Date) - $buildStart).TotalSeconds
Write-Host ""
Write-Host "[OK] Release build completed in $([math]::Round($buildTime, 1))s" -ForegroundColor Green
Write-Host ""

# Find the binary
$binaryPath = "target\release\awclippy-windows.exe"
if (-not (Test-Path $binaryPath)) {
    Write-Host "[X] Release binary not found at $binaryPath" -ForegroundColor Red
    exit 1
}

# Get binary size
$binarySize = (Get-Item $binaryPath).Length
$sizeMB = [math]::Round($binarySize / 1MB, 2)
Write-Host "[OK] Binary size: $sizeMB MB" -ForegroundColor Green
Write-Host "[OK] Binary location: $binaryPath" -ForegroundColor Cyan
Write-Host ""

# Optional installation
if ($Install) {
    Write-Host "[*] Installing to system..." -ForegroundColor Yellow
    
    # Check for admin rights
    $isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    
    if (-not $isAdmin) {
        Write-Host "[!] Installation requires administrator privileges" -ForegroundColor Yellow
        Write-Host "[!] Please run as administrator or manually copy:" -ForegroundColor Yellow
        Write-Host "    $binaryPath" -ForegroundColor Cyan
        Write-Host "    -> C:\Program Files\AWClippy\awclippy-windows.exe" -ForegroundColor Cyan
        Write-Host ""
    } else {
        $installDir = "C:\Program Files\AWClippy"
        
        # Create directory if it doesn't exist
        if (-not (Test-Path $installDir)) {
            New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        }
        
        # Copy binary
        Copy-Item $binaryPath "$installDir\awclippy-windows.exe" -Force
        Write-Host "[OK] Installed to $installDir\awclippy-windows.exe" -ForegroundColor Green
        Write-Host ""
        
        # Create startup shortcut
        Write-Host "[*] Creating startup shortcut..." -ForegroundColor Yellow
        $startupFolder = [Environment]::GetFolderPath("Startup")
        $shortcutPath = Join-Path $startupFolder "AWClippy.lnk"
        
        $WScriptShell = New-Object -ComObject WScript.Shell
        $shortcut = $WScriptShell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = "$installDir\awclippy-windows.exe"
        $shortcut.WorkingDirectory = $installDir
        $shortcut.Description = "AWClippy Universal Clipboard"
        $shortcut.Save()
        
        Write-Host "[OK] Startup shortcut created" -ForegroundColor Green
        Write-Host ""
    }
}

Write-Host "========================================================" -ForegroundColor Green
Write-Host "             BUILD SUCCESSFUL                           " -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Release Binary Details:" -ForegroundColor White
Write-Host "  • Location: $binaryPath" -ForegroundColor Cyan
Write-Host "  • Size: $sizeMB MB" -ForegroundColor Cyan
Write-Host "  • Optimized: Yes (release build with LTO)" -ForegroundColor Cyan
Write-Host ""

if (-not $Install) {
    Write-Host "To install to system, run: .\release.ps1 -Install" -ForegroundColor DarkGray
    Write-Host "Or manually copy the binary to desired location" -ForegroundColor DarkGray
    Write-Host ""
}

Write-Host "To run: .\target\release\awclippy-windows.exe" -ForegroundColor White
Write-Host ""
