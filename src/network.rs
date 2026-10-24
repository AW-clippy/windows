use crate::{clipboard, AppState, log};
use tokio::net::{TcpListener, TcpStream};
use std::net::{SocketAddr, IpAddr};
use tokio::sync::{Mutex, mpsc};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt};

const MAX_CLIPBOARD_SIZE: usize = 1024 * 1024; // 1MB limit
const SERVICE_TYPE: &str = "_awclippy._tcp.local.";

// Cached Android device info
#[allow(dead_code)]
pub struct AndroidDevice {
    pub addr: SocketAddr,
}

pub static ANDROID_DEVICE: Mutex<Option<AndroidDevice>> = Mutex::const_new(None);
pub static ACTIVE_WS_SENDER: Mutex<Option<mpsc::UnboundedSender<Message>>> = Mutex::const_new(None);

#[derive(Debug, Clone)]
pub enum ProtocolMessage {
    Handshake { device_name: String, network_name: String },
    HandshakeAck { device_name: String, network_name: String },
    Ping { device_name: String },
    Pong { device_name: String },
    Clipboard { content: String, device_name: String },
    RawText(String),
}

impl ProtocolMessage {
    pub fn to_string(&self) -> String {
        match self {
            ProtocolMessage::Handshake { device_name, network_name } => {
                format!(
                    r#"{{"type":"HANDSHAKE","device_name":"{}","network_name":"{}"}}"#,
                    json_escape(device_name),
                    json_escape(network_name)
                )
            }
            ProtocolMessage::HandshakeAck { device_name, network_name } => {
                format!(
                    r#"{{"type":"HANDSHAKE_ACK","device_name":"{}","network_name":"{}"}}"#,
                    json_escape(device_name),
                    json_escape(network_name)
                )
            }
            ProtocolMessage::Ping { device_name } => {
                format!(r#"{{"type":"PING","device_name":"{}"}}"#, json_escape(device_name))
            }
            ProtocolMessage::Pong { device_name } => {
                format!(r#"{{"type":"PONG","device_name":"{}"}}"#, json_escape(device_name))
            }
            ProtocolMessage::Clipboard { content, device_name } => {
                format!(
                    r#"{{"type":"CLIPBOARD","content":"{}","device_name":"{}"}}"#,
                    json_escape(content),
                    json_escape(device_name)
                )
            }
            ProtocolMessage::RawText(s) => s.clone(),
        }
    }

    pub fn parse(s: &str) -> ProtocolMessage {
        let trimmed = s.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return ProtocolMessage::RawText(s.to_string());
        }

        let msg_type = extract_json_field(trimmed, "type");
        match msg_type.as_deref() {
            Some("HANDSHAKE") => {
                let device_name = extract_json_field(trimmed, "device_name").unwrap_or_else(|| "Android Device".to_string());
                let network_name = extract_json_field(trimmed, "network_name").unwrap_or_else(|| "Unknown Network".to_string());
                ProtocolMessage::Handshake { device_name, network_name }
            }
            Some("HANDSHAKE_ACK") => {
                let device_name = extract_json_field(trimmed, "device_name").unwrap_or_else(|| "Android Device".to_string());
                let network_name = extract_json_field(trimmed, "network_name").unwrap_or_else(|| "Unknown Network".to_string());
                ProtocolMessage::HandshakeAck { device_name, network_name }
            }
            Some("PING") => {
                let device_name = extract_json_field(trimmed, "device_name").unwrap_or_default();
                ProtocolMessage::Ping { device_name }
            }
            Some("PONG") => {
                let device_name = extract_json_field(trimmed, "device_name").unwrap_or_default();
                ProtocolMessage::Pong { device_name }
            }
            Some("CLIPBOARD") => {
                let content = extract_json_field(trimmed, "content").unwrap_or_default();
                let device_name = extract_json_field(trimmed, "device_name").unwrap_or_else(|| "Android Device".to_string());
                ProtocolMessage::Clipboard { content, device_name }
            }
            _ => ProtocolMessage::RawText(s.to_string()),
        }
    }
}

fn extract_json_field(json: &str, field_name: &str) -> Option<String> {
    let key = format!("\"{}\":", field_name);
    let key_pos = json.find(&key)?;
    let after_key = json[key_pos + key.len()..].trim_start();
    if after_key.starts_with('"') {
        let mut result = String::new();
        let mut chars = after_key[1..].chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        '"' => result.push('"'),
                        '\\' => result.push('\\'),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        'u' => {
                            let mut hex = String::new();
                            for _ in 0..4 {
                                if let Some(h) = chars.next() {
                                    hex.push(h);
                                }
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(unicode_char) = char::from_u32(code) {
                                    result.push(unicode_char);
                                }
                            }
                        }
                        other => {
                            result.push('\\');
                            result.push(other);
                        }
                    }
                }
            } else if c == '"' {
                return Some(result);
            } else {
                result.push(c);
            }
        }
        None
    } else {
        let end_pos = after_key.find([',', '}'])?;
        Some(after_key[..end_pos].trim().to_string())
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

pub fn get_computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Windows PC".to_string())
}

pub fn get_network_name() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
            
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("SSID") && !trimmed.starts_with("BSSID") {
                    if let Some((_, ssid)) = trimmed.split_once(':') {
                        let ssid = ssid.trim();
                        if !ssid.is_empty() {
                            return ssid.to_string();
                        }
                    }
                }
            }
        }
        
        let ps_output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-NetConnectionProfile | Select-Object -ExpandProperty Name)[0]"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
            
        if let Ok(out) = ps_output {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }
    
    "Local Network".to_string()
}

pub async fn start_listener(state: AppState, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    log::debug_log!("WebSocket server listening on port {}", port);

    loop {
        let (stream, addr) = listener.accept().await?;
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(_e) = handle_ws_connection(stream, addr, state_clone).await {
                log::debug_log!("[WEBSOCKET] Connection ended with {}: {}", addr.ip(), _e);
            }
        });
    }
}

async fn handle_ws_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::debug_log!("[WEBSOCKET] Incoming connection from {}", addr.ip());
    
    let ws_stream = accept_async(stream).await
        .map_err(|e| format!("WebSocket handshake error: {}", e))?;
    log::debug_log!("[WEBSOCKET] ✅ Link established with {}", addr.ip());

    let (mut ws_sink, mut ws_reader) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    {
        let mut sender_guard = ACTIVE_WS_SENDER.lock().await;
        *sender_guard = Some(tx.clone());
    }

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(_e) = ws_sink.send(msg).await {
                log::debug_log!("[WEBSOCKET] Sink write error: {}", _e);
                break;
            }
        }
    });

    let target_addr = SocketAddr::new(addr.ip(), state.sync_port);

    while let Some(msg_res) = ws_reader.next().await {
        match msg_res {
            Ok(Message::Binary(payload)) => {
                if payload.len() > MAX_CLIPBOARD_SIZE {
                    log::debug_error!("Payload too large: {} bytes", payload.len());
                    continue;
                }
                if let Ok(text) = state.crypto.decrypt(&payload) {
                    handle_incoming_payload(&text, target_addr, &state, &tx).await;
                }
            }
            Ok(Message::Text(text)) => {
                handle_incoming_payload(&text, target_addr, &state, &tx).await;
            }
            Ok(Message::Ping(data)) => {
                let _ = tx.send(Message::Pong(data));
            }
            Ok(Message::Close(_)) => {
                log::debug_log!("[WEBSOCKET] Peer closed connection");
                break;
            }
            Err(_e) => {
                log::debug_log!("[WEBSOCKET] Link error: {}", _e);
                break;
            }
            _ => {}
        }
    }

    writer_task.abort();

    {
        let mut sender_guard = ACTIVE_WS_SENDER.lock().await;
        *sender_guard = None;
    }
    {
        let mut cached = ANDROID_DEVICE.lock().await;
        *cached = None;
    }
    {
        let mut name_lock = state.android_device_name.lock().await;
        *name_lock = None;
    }
    state.android_connected.store(false, std::sync::atomic::Ordering::SeqCst);
    state.status_dirty.store(true, std::sync::atomic::Ordering::SeqCst);
    log::debug_log!("[WEBSOCKET] Android device disconnected. Transitioned to unsynchronized.");

    Ok(())
}

async fn handle_incoming_payload(
    text: &str,
    target_addr: SocketAddr,
    state: &AppState,
    tx: &mpsc::UnboundedSender<Message>,
) {
    let msg = ProtocolMessage::parse(text);
    match msg {
        ProtocolMessage::Handshake { device_name, network_name } => {
            log::debug_log!("[HANDSHAKE] Handshake from '{}' (Network: {}) at {}", device_name, network_name, target_addr);
            {
                let mut cached = ANDROID_DEVICE.lock().await;
                *cached = Some(AndroidDevice { addr: target_addr });
            }
            {
                let mut name_lock = state.android_device_name.lock().await;
                *name_lock = Some(device_name.clone());
            }
            {
                let mut last_seen_lock = state.last_seen.lock().await;
                *last_seen_lock = Some(std::time::Instant::now());
            }
            state.android_connected.store(true, std::sync::atomic::Ordering::SeqCst);
            state.status_dirty.store(true, std::sync::atomic::Ordering::SeqCst);

            let ack = {
                let net_name = state.network_name.lock().await.clone();
                ProtocolMessage::HandshakeAck {
                    device_name: get_computer_name(),
                    network_name: net_name,
                }
            };
            if let Ok(ack_bytes) = state.crypto.encrypt(&ack.to_string()) {
                let _ = tx.send(Message::Binary(ack_bytes));
                log::debug_log!("[HANDSHAKE] ✅ Synchronized with '{}' and replied with HANDSHAKE_ACK", device_name);
            }
        }
        ProtocolMessage::HandshakeAck { device_name, network_name } => {
            log::debug_log!("[HANDSHAKE] Handshake ACK received from '{}' (Network: {})", device_name, network_name);
            {
                let mut cached = ANDROID_DEVICE.lock().await;
                *cached = Some(AndroidDevice { addr: target_addr });
            }
            {
                let mut name_lock = state.android_device_name.lock().await;
                *name_lock = Some(device_name.clone());
            }
            {
                let mut last_seen_lock = state.last_seen.lock().await;
                *last_seen_lock = Some(std::time::Instant::now());
            }
            state.android_connected.store(true, std::sync::atomic::Ordering::SeqCst);
            state.status_dirty.store(true, std::sync::atomic::Ordering::SeqCst);
            log::debug_log!("[HANDSHAKE] ✅ Synchronized with '{}'", device_name);
        }
        ProtocolMessage::Clipboard { content, device_name } => {
            log::debug_log!("[ANDROID→WINDOWS] Received clipboard from '{}' ({} bytes)", device_name, content.len());
            {
                let mut cached = ANDROID_DEVICE.lock().await;
                *cached = Some(AndroidDevice { addr: target_addr });
            }
            {
                let mut name_lock = state.android_device_name.lock().await;
                *name_lock = Some(device_name);
            }
            {
                let mut last_seen_lock = state.last_seen.lock().await;
                *last_seen_lock = Some(std::time::Instant::now());
            }
            state.android_connected.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = apply_clipboard_content(state, &content).await;
        }
        ProtocolMessage::RawText(raw) => {
            log::debug_log!("[ANDROID→WINDOWS] Received raw clipboard ({} bytes)", raw.len());
            {
                let mut cached = ANDROID_DEVICE.lock().await;
                *cached = Some(AndroidDevice { addr: target_addr });
            }
            {
                let mut last_seen_lock = state.last_seen.lock().await;
                *last_seen_lock = Some(std::time::Instant::now());
            }
            state.android_connected.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = apply_clipboard_content(state, &raw).await;
        }
        _ => {}
    }
}

async fn apply_clipboard_content(state: &AppState, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let text_hash = clipboard::hash_text(text);
    clipboard::update_last_hash(state, text_hash).await;

    let text_clone = text.to_string();
    let result = tokio::task::spawn_blocking(move || {
        clipboard::set_clipboard_text(&text_clone)
    }).await;
    
    match result {
        Ok(Ok(())) => {
            log::debug_log!("[ANDROID→WINDOWS] ✅ Windows clipboard updated!");
            Ok(())
        }
        Ok(Err(e)) => {
            log::debug_error!("[ANDROID→WINDOWS] ❌ Failed to set clipboard: {}", e);
            Err(format!("Clipboard error: {}", e).into())
        }
        Err(e) => {
            log::debug_error!("[ANDROID→WINDOWS] ❌ Task join error: {}", e);
            Err(format!("Task error: {}", e).into())
        }
    }
}

pub async fn broadcast_clipboard(state: &AppState, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::debug_log!("[WINDOWS→ANDROID] Preparing to broadcast clipboard ({} chars)", text.len());
    
    let sender_opt = {
        let guard = ACTIVE_WS_SENDER.lock().await;
        guard.clone()
    };
    
    if let Some(sender) = sender_opt {
        let msg = ProtocolMessage::Clipboard {
            content: text.to_string(),
            device_name: get_computer_name(),
        };
        let encrypted_data = state.crypto.encrypt(&msg.to_string())
            .map_err(|e| format!("Encryption error: {}", e))?;
        
        sender.send(Message::Binary(encrypted_data))
            .map_err(|e| format!("WebSocket send error: {}", e))?;
        
        log::debug_log!("[WINDOWS→ANDROID] ✅ SUCCESS - Sent clipboard over persistent WebSocket link");
        let mut last_seen_lock = state.last_seen.lock().await;
        *last_seen_lock = Some(std::time::Instant::now());
    } else {
        log::debug_log!("[WINDOWS→ANDROID] ⏳ No active WebSocket connection to phone, skipping send");
    }
    Ok(())
}

pub async fn start_heartbeat_monitor(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            // Refresh active network name
            let current_net = get_network_name();
            {
                let mut net_lock = state.network_name.lock().await;
                if *net_lock != current_net {
                    log::debug_log!("Network changed to: {}", current_net);
                    *net_lock = current_net;
                }
            }
        }
    });
}

pub static MDNS_DAEMON: Mutex<Option<ServiceDaemon>> = Mutex::const_new(None);

pub async fn get_or_create_mdns() -> Result<ServiceDaemon, Box<dyn std::error::Error + Send + Sync>> {
    let mut guard = MDNS_DAEMON.lock().await;
    if let Some(ref daemon) = *guard {
        Ok(daemon.clone())
    } else {
        let daemon = ServiceDaemon::new()?;
        *guard = Some(daemon.clone());
        Ok(daemon)
    }
}

pub async fn trigger_resync(_state: AppState) {
    log::debug_log!("Triggering re-sync with Android...");
    if let Ok(mdns) = get_or_create_mdns().await {
        let _ = mdns.browse(SERVICE_TYPE);
    }
}

/// Start mDNS service advertisement for Windows
pub async fn start_mdns_service(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mdns = get_or_create_mdns().await?;
    let local_ip = get_local_ip().await?;
    let hostname = format!("awclippy-windows-{}.local.", local_ip.to_string().replace('.', "-"));
    let computer_name = get_computer_name();
    let network_name = get_network_name();
    
    let mut properties = std::collections::HashMap::new();
    properties.insert("name".to_string(), computer_name.clone());
    properties.insert("net".to_string(), network_name.clone());
    properties.insert("v".to_string(), "2".to_string());
    
    let service_info = ServiceInfo::new(
        SERVICE_TYPE,
        "awclippy-windows",
        &hostname,
        local_ip,
        port,
        Some(properties),
    )?
    .enable_addr_auto();
    
    mdns.register(service_info)?;
    log::debug_log!("mDNS service advertising as '{}' ('{}') on {}:{}", hostname, computer_name, local_ip, port);
    
    Ok(())
}

/// Start mDNS browser to log discovered Android devices
pub async fn start_mdns_browser(_state: AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mdns = get_or_create_mdns().await?;
    let receiver = mdns.browse(SERVICE_TYPE)?;
    
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv_async().await {
            match event {
                mdns_sd::ServiceEvent::ServiceResolved(info) => {
                    let fullname = info.get_fullname();
                    if fullname.contains("android") {
                        log::debug_log!("Discovered Android device via mDNS: {}", fullname);
                    }
                }
                mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                    if fullname.contains("android") {
                        log::debug_log!("Android device mDNS service removed: {}", fullname);
                    }
                }
                _ => {}
            }
        }
    });
    
    Ok(())
}

async fn get_local_ip() -> Result<IpAddr, Box<dyn std::error::Error + Send + Sync>> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip())
}
