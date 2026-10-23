# AW Clippy Windows Installer

This directory contains the Windows Installer (MSI) configuration for AW Clippy.

## Prerequisites

### Install WiX Toolset

Download and install WiX Toolset v3.11 or newer from:
- **Official Site**: https://wixtoolset.org/releases/
- **Direct Download**: https://github.com/wixtoolset/wix3/releases

Choose the `.exe` installer for your system.

## Building the Installer

### Using PowerShell Script (Recommended)

```powershell
# From the windows directory
.\build-installer.ps1
```

This will:
1. Build the release binary with Cargo
2. Prepare icon files
3. Compile the WiX source
4. Create `installer/AWClippy-Setup.msi`

### Manual Build

If you prefer to build manually:

```powershell
# 1. Build the release binary
cargo build --release

# 2. Compile WiX source (adjust WiX path if needed)
& "C:\Program Files (x86)\WiX Toolset v3.11\bin\candle.exe" -arch x64 -out installer\awclippy.wixobj installer\main.wxs

# 3. Create MSI
& "C:\Program Files (x86)\WiX Toolset v3.11\bin\light.exe" -ext WixUIExtension -out installer\AWClippy-Setup.msi installer\awclippy.wixobj
```

## What the Installer Does

### Installation
- Installs `awclippy-windows.exe` to `Program Files\AWClippy\`
- Creates a Start Menu shortcut
- Adds AW Clippy to Windows startup (auto-start)
- Registers in Windows "Add or Remove Programs"
- Copies `.env.example` for configuration reference

### Uninstallation
- Stops the running AW Clippy process
- Removes all installed files
- Removes Start Menu shortcut
- Removes startup registry entry
- Cleans up all registry entries
- Shows proper uninstall progress in Windows

## User Instructions

### Installing

1. Double-click `AWClippy-Setup.msi`
2. Follow the installation wizard
3. AW Clippy will start automatically with Windows

### Configuring

After installation, configure the shared secret:

1. Navigate to `C:\Program Files\AWClippy\`
2. Copy `.env.example` to `.env`
3. Edit `.env` and set your `AWCLIPPY_SHARED_SECRET`
4. Restart AW Clippy from the Start Menu or reboot

### Uninstalling

**Method 1: Windows Settings (Recommended)**
1. Open Windows Settings
2. Go to Apps > Installed apps (or Apps & features)
3. Find "AW Clippy"
4. Click "..." menu → Uninstall
5. Follow the prompts

**Method 2: Control Panel**
1. Open Control Panel
2. Go to Programs > Programs and Features
3. Find "AW Clippy"
4. Right-click → Uninstall

**Method 3: MSI Directly**
1. Right-click `AWClippy-Setup.msi`
2. Select "Uninstall"

## Installer Features

✅ **Clean Installation**
- Professional MSI installer
- Proper Windows integration
- Version upgrade support

✅ **Auto-Start**
- Automatically starts with Windows
- Runs in system tray
- No console window

✅ **Easy Uninstall**
- Complete removal of all files
- Registry cleanup
- No leftover traces

✅ **Upgrade Support**
- Automatically removes old versions
- Preserves user configuration
- Seamless updates

## Troubleshooting

### "WiX Toolset not found"

Install WiX from https://wixtoolset.org/releases/ and try again.

### "Failed to stop process during uninstall"

The app might not be running. This is safe to ignore - the uninstaller will proceed.

### "Access denied" during installation

Run the MSI as Administrator:
1. Right-click `AWClippy-Setup.msi`
2. Select "Run as administrator"

### Files not removed after uninstall

If you manually created `.env` or other files, they may remain after uninstall. This is intentional to preserve your configuration for reinstallation.

## Customization

### Changing Version

Edit `windows/Cargo.toml`:
```toml
[package]
version = "0.2.0"
```

The installer will automatically use this version.

### Changing Install Location

Users can choose a custom installation directory during the installation wizard.

### Adding More Files

Edit `installer/main.wxs` and add more `<File>` components as needed.

## Distribution

The resulting `AWClippy-Setup.msi` is a standalone installer that can be:
- Uploaded to GitHub Releases
- Shared directly with users
- Deployed via software management tools
- Installed silently: `msiexec /i AWClippy-Setup.msi /quiet`

## Silent Installation (IT Admins)

For automated deployment:

```powershell
# Silent install
msiexec /i AWClippy-Setup.msi /quiet /norestart

# Silent uninstall
msiexec /x AWClippy-Setup.msi /quiet /norestart

# Install with logging
msiexec /i AWClippy-Setup.msi /quiet /l*v install.log
```
