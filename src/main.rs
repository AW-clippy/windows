#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod crypto;
mod network;
mod tray;
mod ui;
mod log;
mod win32_ffi;

use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::atomic::Ordering;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, TranslateMessage, PeekMessageW, MSG, PM_REMOVE, WM_QUIT,
};

fn load_config() -> (Vec<u8>, u16, u16) {
    // 1. Try to load runtime .env if located next to executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let env_in_exe_dir = exe_dir.join(".env");
            if env_in_exe_dir.exists() {
                dotenvy::from_path(&env_in_exe_dir).ok();
            }
        }
    }
    // 2. Try current working directory .env
    dotenvy::dotenv().ok();
    
    // Get shared secret from environment or fallback to compile-time baked secret from .env
    let default_compiled_secret = option_env!("COMPILED_AWCLIPPY_SHARED_SECRET")
        .unwrap_or("CHANGE_THIS_TO_YOUR_SECRET_KEY!!");

    let secret_str = std::env::var("AWCLIPPY_SHARED_SECRET")
        .unwrap_or_else(|_| default_compiled_secret.to_string());
    
    // Derive uniform 32-byte AES key using SHA-256
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(secret_str.as_bytes());
    let secret = hasher.finalize().to_vec();
    
    let default_discovery_port: u16 = option_env!("COMPILED_AWCLIPPY_DISCOVERY_PORT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(48899);

    let discovery_port = std::env::var("AWCLIPPY_DISCOVERY_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_discovery_port);
    
    let default_sync_port: u16 = option_env!("COMPILED_AWCLIPPY_SYNC_PORT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(48900);

    let sync_port = std::env::var("AWCLIPPY_SYNC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_sync_port);
    
    (secret, discovery_port, sync_port)
}

#[derive(Clone)]
pub struct AppState {
    pub last_clipboard_hash: Arc<Mutex<Option<u64>>>,
    pub crypto: crypto::ClipboardCrypto,
    pub sync_port: u16,
    #[allow(dead_code)]
    pub discovery_port: u16,
    pub paused: Arc<std::sync::atomic::AtomicBool>,
    pub android_device_name: Arc<Mutex<Option<String>>>,
    pub android_connected: Arc<std::sync::atomic::AtomicBool>,
    pub network_name: Arc<Mutex<String>>,
    pub last_seen: Arc<Mutex<Option<std::time::Instant>>>,
    pub status_dirty: Arc<std::sync::atomic::AtomicBool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--exit" || a == "--kill" || a == "--stop") {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "awclippy-windows.exe", "/T"])
            .output();
        return Ok(());
    }

    // Ensure only one instance of AWClippy runs at a time
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::core::w;

    let _single_instance = unsafe {
        let m = CreateMutexW(None, true, w!("Global\\AWClippy_SingleInstance_Mutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log::debug_log!("AWClippy is already running in background.");
            return Ok(());
        }
        m
    };

    log::debug_log!("AW Clippy Windows Client Starting...");
    
    // Load configuration
    let (shared_secret, discovery_port, sync_port) = load_config();
    
    log::debug_log!("Configuration loaded:");
    log::debug_log!("  Discovery Port: {}", discovery_port);
    log::debug_log!("  Sync Port: {}", sync_port);
    
    let initial_network = network::get_network_name();
    log::debug_log!("Initial Network Name: {}", initial_network);
    
    let state = AppState {
        last_clipboard_hash: Arc::new(Mutex::new(None)),
        crypto: crypto::ClipboardCrypto::new(&shared_secret.try_into().unwrap()),
        sync_port,
        discovery_port,
        paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        android_device_name: Arc::new(Mutex::new(None)),
        android_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        network_name: Arc::new(Mutex::new(initial_network.clone())),
        last_seen: Arc::new(Mutex::new(None)),
        status_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    // Start network listener
    let network_state = state.clone();
    let sync_port = state.sync_port;
    tokio::spawn(async move {
        if let Err(_e) = network::start_listener(network_state, sync_port).await {
            log::debug_error!("Network listener error: {}", _e);
        }
    });

    // Start mDNS service advertisement
    if let Err(_e) = network::start_mdns_service(sync_port).await {
        log::debug_error!("mDNS service error: {}", _e);
    }
    
    // Start mDNS browser to discover Android devices
    let browser_state = state.clone();
    if let Err(_e) = network::start_mdns_browser(browser_state).await {
        log::debug_error!("mDNS browser error: {}", _e);
    }

    // Start network and heartbeat monitor
    network::start_heartbeat_monitor(state.clone()).await;

    // Initialize system tray with comprehensive menu
    let tray_handle = match tray::TrayMenu::new(state.paused.clone()) {
        Ok(tray) => {
            log::debug_log!("✅ System tray initialized successfully");
            tray.update_status(&initial_network, None);
            Some(tray)
        }
        Err(_e) => {
            log::debug_error!("⚠️  Could not create system tray: {}", _e);
            log::debug_error!("    App will continue running without tray icon");
            None
        }
    };

    // Clone flags for the message loop
    let settings_requested = tray_handle.as_ref().map(|t| Arc::clone(&t.settings_requested));
    let resync_requested = tray_handle.as_ref().map(|t| Arc::clone(&t.resync_requested));
    
    // Create settings window
    let settings_window = match ui::SettingsWindow::new() {
        Ok(win) => Some(win),
        Err(_e) => {
            log::debug_error!("⚠️  Could not create settings window: {}", _e);
            None
        }
    };

    // Start clipboard monitoring on main thread (Windows requirement)
    let clipboard_state = state.clone();
    clipboard::monitor_clipboard(clipboard_state).await.map_err(|e| e.to_string())?;

    // Show system notification indicating app is active in system tray
    if let Some(hwnd) = clipboard::get_hwnd() {
        tray::show_startup_notification(hwnd);
    }

    // Message loop for Windows - use PeekMessageW for non-blocking message processing
    unsafe {
        let mut msg = MSG::default();
        let mut last_status_update = std::time::Instant::now();
        
        loop {
            // Check if settings window was requested
            if let (Some(ref flag), Some(ref settings)) = (&settings_requested, &settings_window) {
                if flag.swap(false, Ordering::SeqCst) {
                    let net = state.network_name.try_lock().map(|l| l.clone()).unwrap_or_else(|_| "Local Network".to_string());
                    let phone = state.android_device_name.try_lock().ok().and_then(|l| l.clone());
                    settings.show(&net, phone.as_deref());
                }
            }
            
            // Check if re-sync was requested from tray menu
            if let Some(ref resync_flag) = resync_requested {
                if resync_flag.swap(false, Ordering::SeqCst) {
                    let resync_state = state.clone();
                    tokio::spawn(async move {
                        network::trigger_resync(resync_state).await;
                    });
                }
            }
            
            // Check for tray icon events (mouse hover/click) to refresh status immediately
            if let Ok(_event) = tray_icon::TrayIconEvent::receiver().try_recv() {
                if let (Ok(device_name_lock), Ok(net_name_lock)) = (state.android_device_name.try_lock(), state.network_name.try_lock()) {
                    if let Some(ref tray) = tray_handle {
                        tray.update_status(&net_name_lock, device_name_lock.as_deref());
                    }
                }
            }

            // Update tray status if status became dirty OR periodically (every 300ms)
            let dirty = state.status_dirty.swap(false, Ordering::SeqCst);
            if tray_handle.is_some() && (dirty || last_status_update.elapsed() > std::time::Duration::from_millis(300)) {
                if let (Ok(device_name_lock), Ok(net_name_lock)) = (state.android_device_name.try_lock(), state.network_name.try_lock()) {
                    if let Some(ref tray) = tray_handle {
                        tray.update_status(&net_name_lock, device_name_lock.as_deref());
                    }
                }
                last_status_update = std::time::Instant::now();
            }
            
            // Process all pending window messages immediately
            let mut had_messages = false;
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                had_messages = true;
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if msg.message == WM_QUIT {
                break;
            }
            if !had_messages {
                // Short yield when idle
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
    }

    // Keep tray alive
    drop(tray_handle);

    Ok(())
}
