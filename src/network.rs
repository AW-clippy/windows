use crate::{clipboard, AppState, log};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::net::{SocketAddr, IpAddr};
use tokio::sync::Mutex;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::time::Duration;

const MAX_CLIPBOARD_SIZE: usize = 1024 * 1024; // 1MB limit
const SERVICE_TYPE: &str = "_awclippy._tcp.local.";

// Cached Android device info
struct AndroidDevice {
    addr: SocketAddr,
}

static ANDROID_DEVICE: Mutex<Option<AndroidDevice>> = Mutex::const_new(None);

pub async fn start_listener(state: AppState, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    log::debug_log!("Listening for clipboard sync on port {}", port);

    loop {
        let (stream, addr) = listener.accept().await?;
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(_e) = handle_connection(stream, addr, state_clone).await {
                if _e.to_string() != "early eof" && !_e.to_string().contains("completed successfully") {
                    log::debug_error!("Connection error from {}: {}", addr, _e);
                }
            }
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    _addr: SocketAddr,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    log::debug_log!("[ANDROID→WINDOWS] Incoming connection from {}", _addr);
    
    // Read length prefix (4 bytes) with 5-second timeout
    let mut len_buf = [0u8; 4];
    match tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut len_buf)).await {
        Ok(Ok(_)) => {},
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Ok(());
        }
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => return Err("Connection timed out waiting for length prefix".into()),
    }
    
    let len = u32::from_be_bytes(len_buf) as usize;
    log::debug_log!("[ANDROID→WINDOWS] Receiving {} bytes from Android...", len);

    if len > MAX_CLIPBOARD_SIZE {
        return Err("Clipboard data too large".into());
    }

    // Read encrypted payload with 5-second timeout
    let mut payload = vec![0u8; len];
    match tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut payload)).await {
        Ok(Ok(_)) => {},
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => return Err("Connection timed out reading payload".into()),
    }
    log::debug_log!("[ANDROID→WINDOWS] Received complete payload, decrypting...");

    // Decrypt
    let text = state.crypto.decrypt(&payload)?;
    log::debug_log!("[ANDROID→WINDOWS] ✅ Decrypted successfully ({} bytes)", text.len());
    
    // Calculate hash and update to prevent echo
    let text_hash = clipboard::hash_text(&text);
    clipboard::update_last_hash(&state, text_hash).await;

    // Set clipboard
    log::debug_log!("[ANDROID→WINDOWS] Setting Windows clipboard...");
    
    // Spawn a blocking task for clipboard operation
    // Windows clipboard requires a thread with message loop context
    let text_clone = text.clone();
    let result = tokio::task::spawn_blocking(move || {
        clipboard::set_clipboard_text(&text_clone)
    }).await;
    
    match result {
        Ok(Ok(())) => {
            log::debug_log!("[ANDROID→WINDOWS] ✅ Windows clipboard updated!");
        }
        Ok(Err(e)) => {
            log::debug_error!("[ANDROID→WINDOWS] ❌ Failed to set clipboard: {}", e);
            return Err(format!("Clipboard error: {}", e).into());
        }
        Err(e) => {
            log::debug_error!("[ANDROID→WINDOWS] ❌ Task join error: {}", e);
            return Err(format!("Task error: {}", e).into());
        }
    }

    Ok(())
}

pub async fn broadcast_clipboard(encrypted_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    log::debug_log!("[WINDOWS→ANDROID] broadcast_clipboard called with {} bytes", encrypted_data.len());
    
    let broadcast_addr = match get_android_address().await {
        Ok(addr) => {
            log::debug_log!("[WINDOWS→ANDROID] Target Android address: {}", addr);
            addr
        },
        Err(_e) => {
            log::debug_log!("[WINDOWS→ANDROID] ❌ FAILED - No Android device found: {}", _e);
            return Ok(());
        }
    };
    
    log::debug_log!("[WINDOWS→ANDROID] Connecting to Android at {}...", broadcast_addr);
    
    match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(broadcast_addr)).await {
        Ok(Ok(mut stream)) => {
            log::debug_log!("[WINDOWS→ANDROID] Connected! Sending {} bytes...", encrypted_data.len());
            
            // Send length prefix and payload with 5-second timeout
            let len = encrypted_data.len() as u32;
            let write_result = tokio::time::timeout(Duration::from_secs(5), async {
                stream.write_all(&len.to_be_bytes()).await?;
                stream.write_all(encrypted_data).await?;
                stream.flush().await?;
                Ok::<(), std::io::Error>(())
            }).await;

            match write_result {
                Ok(Ok(())) => {
                    log::debug_log!("[WINDOWS→ANDROID] ✅ SUCCESS - Sent clipboard to Android");
                }
                Ok(Err(_e)) => {
                    log::debug_error!("[WINDOWS→ANDROID] ❌ Write error: {}", _e);
                }
                Err(_) => {
                    log::debug_error!("[WINDOWS→ANDROID] ❌ Send timed out after 5 seconds");
                }
            }
            Ok(())
        }
        Ok(Err(e)) => {
            if e.kind() != std::io::ErrorKind::ConnectionRefused {
                log::debug_error!("[WINDOWS→ANDROID] ❌ FAILED - Connection error: {}", e);
            } else {
                log::debug_log!("[WINDOWS→ANDROID] ❌ Android device not available (connection refused)");
            }
            Ok(())
        }
        Err(_) => {
            log::debug_log!("[WINDOWS→ANDROID] ❌ Connection timed out to {}", broadcast_addr);
            Ok(())
        }
    }
}

async fn get_android_address() -> Result<SocketAddr, String> {
    let cached = ANDROID_DEVICE.lock().await;
    if let Some(device) = cached.as_ref() {
        return Ok(device.addr);
    }
    Err("No Android device found yet".to_string())
}

/// Start mDNS service advertisement for Windows
pub async fn start_mdns_service(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mdns = ServiceDaemon::new()?;
    
    // Get local IP address (prefer Wi-Fi over Ethernet/VirtualBox)
    let local_ip = get_local_ip().await?;
    let hostname = format!("awclippy-windows-{}.local", local_ip.to_string().replace('.', "-"));
    
    let service_info = ServiceInfo::new(
        SERVICE_TYPE,
        "awclippy-windows",
        &hostname,
        local_ip,
        port,
        None,
    )?
    .enable_addr_auto();
    
    mdns.register(service_info)?;
    log::debug_log!("mDNS service advertising as '{}' on {}:{}", hostname, local_ip, port);
    
    // Keep advertising
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    
    Ok(())
}

/// Start mDNS browser to discover Android devices
pub async fn start_mdns_browser(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let mdns = ServiceDaemon::new()?;
    
    let receiver = mdns.browse("_awclippy._tcp.local.")?;
    
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv_async().await {
            match event {
                mdns_sd::ServiceEvent::ServiceResolved(info) => {
                    // Check if this is an Android device
                    let fullname = info.get_fullname();
                    if fullname.contains("android") {
                        // Extract device name from service name
                        // Format: "awclippy-android-192-168-1-100._awclippy._tcp.local."
                        let device_name = info.get_fullname()
                            .split('.')
                            .next()
                            .unwrap_or("Android Device")
                            .strip_prefix("awclippy-android-")
                            .map(|s| {
                                // Convert IP dashes to dots
                                s.replace('-', ".")
                            })
                            .unwrap_or_else(|| "Android Device".to_string());
                        
                        // Find first IPv4 address (skip IPv6 link-local)
                        if let Some(addr) = info.get_addresses().iter()
                            .find(|a| a.is_ipv4()) {
                            let socket_addr = SocketAddr::new(*addr, info.get_port());
                            let mut cached = ANDROID_DEVICE.lock().await;
                            *cached = Some(AndroidDevice {
                                addr: socket_addr,
                            });
                            
                            // Update app state
                            {
                                let mut name_lock = state.android_device_name.lock().await;
                                *name_lock = Some(device_name.clone());
                            }
                            state.android_connected.store(true, std::sync::atomic::Ordering::SeqCst);
                            
                            log::debug_log!("Discovered Android device '{}' via mDNS at {}", device_name, socket_addr);
                        }
                    }
                }
                mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                    if fullname.contains("android") {
                        let mut cached = ANDROID_DEVICE.lock().await;
                        *cached = None;
                        
                        // Update app state
                        {
                            let mut name_lock = state.android_device_name.lock().await;
                            *name_lock = None;
                        }
                        state.android_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                        
                        log::debug_log!("Android device disconnected");
                    }
                }
                _ => {}
            }
        }
    });
    
    Ok(())
}

/// Get the best local IP address (works on all languages)
async fn get_local_ip() -> Result<IpAddr, Box<dyn std::error::Error>> {
    use std::net::UdpSocket;
    
    // Language-independent method: connect to external IP to determine routing
    // This doesn't actually send packets, just determines which interface would be used
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let local_addr = socket.local_addr()?;
    
    Ok(local_addr.ip())
}
