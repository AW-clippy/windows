# AW Clippy - Windows

Lightweight clipboard sync between Windows and Android. Runs in the system tray with minimal resource usage.

## Features

- 🔄 Automatic instantaneous clipboard sync with Android
- ⚡ **Lightweight WebSocket Server** (tokio-tungstenite async full-duplex connection)
- 🚫 **Zero Polling**: No periodic pings or connection churn — events push immediately
- 🔒 Encrypted transmission (AES-256-GCM)
- 🌐 Zero-configuration auto-discovery via mDNS (`mdns-sd`)
- 💾 Extremely low resource usage (~a few MB RAM)
- 🎨 Native Windows system tray interface with Win32 clipboard format listener

## Requirements

- Windows 10/11
- Rust 1.70+ (for building)
- Same WiFi network / local subnet as Android device

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
- `AWCLIPPY_SYNC_PORT` - WebSocket sync port (default: 48900)

To change these settings, you must rebuild both Windows and Android apps.

## How It Works

1. **mDNS Service Advertising**: Windows broadcasts its presence on the local network as `_awclippy._tcp.local.` using `mdns-sd`.
2. **Persistent WebSocket Server**: Windows listens on port 48900. When the Android client connects, a single full-duplex WebSocket link is established using `tokio-tungstenite`.
3. **Instantaneous Event Push**: 
   - When text is copied on Windows, a Win32 clipboard format listener (`WM_CLIPBOARDUPDATE`) catches the event instantly, encrypts the text with AES-256-GCM, and broadcasts it across the existing WebSocket connection without establishing a new connection.
   - When text arrives from Android over the WebSocket, Windows decrypts it and updates the local clipboard.
4. **No Polling**: The link remains idle between clipboard events, consuming no unnecessary CPU or network bandwidth.

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
