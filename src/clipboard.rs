use crate::{network, AppState, log, win32_ffi};
use std::sync::Arc;
use tokio::sync::Mutex;
use windows::Win32::Foundation::HWND;

// Store the monitor globally so we can access its HWND
static MONITOR: Mutex<Option<Arc<win32_ffi::ClipboardMonitor>>> = Mutex::const_new(None);

/// Initialize clipboard monitoring
/// 
/// Sets up a Windows clipboard listener that triggers on any clipboard change.
/// The callback is invoked asynchronously in the tokio runtime.
pub async fn monitor_clipboard(state: AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state_arc = Arc::new(Mutex::new(state));
    
    // Create clipboard monitor with callback
    let state_for_callback = state_arc.clone();
    let monitor = win32_ffi::ClipboardMonitor::new(move || {
        let state = state_for_callback.clone();
        
        // Spawn async task to handle clipboard change
        tokio::spawn(async move {
            // Short delay to ensure source application has finalized clipboard data
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            
            if let Err(e) = handle_clipboard_change(state).await {
                log::debug_error!("Clipboard sync error: {}", e);
            }
        });
    })?;
    
    log::debug_log!("Native clipboard format listener active (AddClipboardFormatListener)");
    
    // Store monitor in global so we can access its HWND
    let monitor_arc = Arc::new(monitor);
    *MONITOR.lock().await = Some(monitor_arc);
    
    Ok(())
}

/// Get the clipboard monitor window handle
pub fn get_hwnd() -> Option<HWND> {
    // Use try_lock to avoid blocking, return None if can't acquire lock
    MONITOR.try_lock().ok()?.as_ref().map(|m| m.hwnd())
}

/// Handle a clipboard change event
async fn handle_clipboard_change(state_arc: Arc<Mutex<AppState>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read clipboard using safe FFI wrapper
    let text = win32_ffi::read_clipboard()?;
    
    if text.is_empty() {
        return Ok(());
    }

    // Check if sync is paused
    let state = state_arc.lock().await;
    if state.paused.load(std::sync::atomic::Ordering::SeqCst) {
        log::debug_log!("Clipboard sync is paused, skipping...");
        return Ok(());
    }

    let text_hash = hash_text(&text);
    
    let mut last_hash = state.last_clipboard_hash.lock().await;
    
    // Check if this is the same text we just received from network (echo prevention)
    if let Some(prev_hash) = *last_hash {
        if prev_hash == text_hash {
            log::debug_log!("Ignoring - this is an echo from Android");
            return Ok(());
        }
    }
    
    *last_hash = Some(text_hash);
    drop(last_hash);
    
    // Broadcast clipboard via network protocol
    network::broadcast_clipboard(&state, &text).await?;
    
    log::debug_log!("Clipboard synced: {} bytes", text.len());
    
    Ok(())
}

/// Set clipboard text (public API)
/// 
/// This is called when receiving clipboard data from Android.
/// Uses safe FFI wrapper with automatic retry logic.
pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    win32_ffi::write_clipboard(text)
}

/// Compute SHA-256 hash of text for echo detection
pub fn hash_text(text: &str) -> u64 {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    u64::from_le_bytes(result[0..8].try_into().unwrap())
}

/// Update the last clipboard hash (for echo prevention)
pub async fn update_last_hash(state: &AppState, hash: u64) {
    let mut last_hash = state.last_clipboard_hash.lock().await;
    *last_hash = Some(hash);
}
