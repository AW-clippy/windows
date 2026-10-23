mod clipboard;
mod crypto;
mod network;
mod tray;
mod ui;
mod log;

use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::atomic::Ordering;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, TranslateMessage, PeekMessageW, MSG, PM_REMOVE, WM_QUIT,
};

fn load_config() -> (Vec<u8>, u16, u16) {
    // Try to load .env file
    dotenvy::dotenv().ok();
    
    // Get shared secret from environment or use default
    let secret_str = std::env::var("AWCLIPPY_SHARED_SECRET")
        .unwrap_or_else(|_| {
            log::debug_error!("Warning: AWCLIPPY_SHARED_SECRET not set in .env file");
            log::debug_error!("Using default key - CHANGE THIS FOR SECURITY!");
            "CHANGE_THIS_TO_YOUR_SECRET_KEY!!".to_string()
        });
    
    // Derive uniform 32-byte AES key using SHA-256
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(secret_str.as_bytes());
    let secret = hasher.finalize().to_vec();
    
    let discovery_port = std::env::var("AWCLIPPY_DISCOVERY_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(48899);
    
    let sync_port = std::env::var("AWCLIPPY_SYNC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(48900);
    
    (secret, discovery_port, sync_port)
}

#[derive(Clone)]
pub struct AppState {
    last_clipboard_hash: Arc<Mutex<Option<u64>>>,
    crypto: crypto::ClipboardCrypto,
    sync_port: u16,
    #[allow(dead_code)]
    discovery_port: u16,
    pub paused: Arc<std::sync::atomic::AtomicBool>,
    pub android_device_name: Arc<Mutex<Option<String>>>,
    pub android_connected: Arc<std::sync::atomic::AtomicBool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::debug_log!("AW Clippy Windows Client Starting...");
    
    // Load configuration
    let (shared_secret, discovery_port, sync_port) = load_config();
    
    log::debug_log!("Configuration loaded:");
    log::debug_log!("  Discovery Port: {}", discovery_port);
    log::debug_log!("  Sync Port: {}", sync_port);
    
    let state = AppState {
        last_clipboard_hash: Arc::new(Mutex::new(None)),
        crypto: crypto::ClipboardCrypto::new(&shared_secret.try_into().unwrap()),
        sync_port,
        discovery_port,
        paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        android_device_name: Arc::new(Mutex::new(None)),
        android_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    // Start network listener
    let network_state = state.clone();
    let sync_port = state.sync_port;
    tokio::spawn(async move {
        if let Err(e) = network::start_listener(network_state, sync_port).await {
            log::debug_error!("Network listener error: {}", e);
        }
    });

    // Start mDNS service advertisement
    if let Err(e) = network::start_mdns_service(sync_port).await {
        log::debug_error!("mDNS service error: {}", e);
    }
    
    // Start mDNS browser to discover Android devices
    let browser_state = state.clone();
    if let Err(e) = network::start_mdns_browser(browser_state).await {
        log::debug_error!("mDNS browser error: {}", e);
    }

    // Initialize system tray with comprehensive menu
    let tray_handle = match tray::TrayMenu::new(state.paused.clone()) {
        Ok(tray) => {
            log::debug_log!("✅ System tray initialized successfully");
            Some(tray)
        }
        Err(e) => {
            log::debug_error!("⚠️  Could not create system tray: {}", e);
            log::debug_error!("    App will continue running without tray icon");
            None
        }
    };

    // Clone the settings_requested flag for the message loop
    let settings_requested = tray_handle.as_ref().map(|t| Arc::clone(&t.settings_requested));
    
    // Create settings window
    let settings_window = match ui::SettingsWindow::new() {
        Ok(win) => Some(win),
        Err(e) => {
            log::debug_error!("⚠️  Could not create settings window: {}", e);
            None
        }
    };

    // Start clipboard monitoring on main thread (Windows requirement)
    let clipboard_state = state.clone();
    clipboard::monitor_clipboard(clipboard_state).await?;

    // Message loop for Windows - use PeekMessageW for non-blocking message processing
    unsafe {
        let mut msg = MSG::default();
        let mut last_status_update = std::time::Instant::now();
        
        loop {
            // Check if settings window was requested
            if let (Some(ref flag), Some(ref settings)) = (&settings_requested, &settings_window) {
                if flag.swap(false, Ordering::SeqCst) {
                    settings.show();
                }
            }
            
            // Update tray status periodically (every 2 seconds)
            if tray_handle.is_some() && last_status_update.elapsed() > std::time::Duration::from_secs(2) {
                // Try to get the device name without blocking
                if let Ok(device_name_lock) = state.android_device_name.try_lock() {
                    if let Some(ref tray) = tray_handle {
                        tray.update_status(device_name_lock.as_deref());
                    }
                }
                last_status_update = std::time::Instant::now();
            }
            
            // Use PeekMessageW for non-blocking message processing
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                // Brief sleep to avoid busy-waiting, but check status periodically
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    // Keep tray alive
    drop(tray_handle);

    Ok(())
}
