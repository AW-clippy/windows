# AW Clippy - Windows

Lightweight clipboard sync between Windows and Android. Runs in the system tray with minimal resource usage.

## Features

- 🔄 Automatic clipboard sync with Android
- 🔒 Encrypted transmission (AES-256-GCM)
- 🌐 Auto-discovery via mDNS
- 💾 Extremely low resource usage
- 🎨 System tray interface

## Requirements

- Windows 10/11
- Rust 1.70+ (for building)
- Same WiFi network as Android device

## Quick Start

1. **Configure**
   ```bash
   copy .env.example .env
   ```
   Edit `.env` and set your shared secret (must match Android app)

2. **Build**
   ```bash
   cargo build --release
   ```

3. **Run**
   ```bash
   .\target\release\awclippy-windows.exe
   ```

The app will appear in your system tray. Right-click for options.

## Configuration

**Important:** Configuration settings are compiled into the application at build time.

Edit `.env` file **before building**:

- `AWCLIPPY_SHARED_SECRET` - 32-byte encryption key (must match Android)
- `AWCLIPPY_DISCOVERY_PORT` - mDNS discovery port (default: 48899)
- `AWCLIPPY_SYNC_PORT` - Clipboard sync port (default: 48900)

To change these settings, you must rebuild both Windows and Android apps.

## Settings

Right-click the tray icon → Settings to:
- View connection status
- Enable/disable "Start with Windows"

## Troubleshooting

- **Not connecting?** Check Windows Firewall and ensure both devices are on the same WiFi
- **Ports blocked?** Verify ports 48899 and 48900 are available
- **Need different ports?** Edit `.env` and rebuild both apps

## Building from Source

```bash
# Debug build
cargo build

# Release build (optimized for size)
cargo build --release
```

## Security

All clipboard data is encrypted using AES-256-GCM with a shared secret. Set a strong 32-character secret in your `.env` file.
