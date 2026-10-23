use crate::{network, AppState, log};
use std::ptr;
use std::sync::Arc;
use tokio::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HANDLE, HGLOBAL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::DataExchange::{
    GetClipboardData, OpenClipboard, 
    CloseClipboard, IsClipboardFormatAvailable, SetClipboardData, EmptyClipboard,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowsHookExW, CallNextHookEx,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
    HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_C};
use windows::core::w;

const CF_UNICODETEXT: u32 = 13;

static mut APP_STATE: Option<Arc<Mutex<AppState>>> = None;
static mut KEYBOARD_HOOK: Option<HHOOK> = None;

pub async fn monitor_clipboard(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        APP_STATE = Some(Arc::new(Mutex::new(state)));

        let h_instance = GetModuleHandleW(None)?;
        
        let class_name = w!("AWClippyMonitorClass");
        
        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: h_instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        RegisterClassW(&wnd_class);

        let _hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("AWClippy Monitor"),
            WINDOW_STYLE::default(),
            0, 0, 0, 0,
            None,
            None,
            h_instance,
            None,
        )?;

        // Install keyboard hook to detect Ctrl+C
        KEYBOARD_HOOK = Some(SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), h_instance, 0)?);

        log::debug_log!("Clipboard monitoring active (Ctrl+C detection only)");
        
        // No need for a separate message loop thread - the main loop handles messages
    }

    Ok(())
}

unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 && (w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN) {
        let kb_struct = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        
        // Check if 'C' key was pressed
        if kb_struct.vkCode == VK_C.0 as u32 {
            // Check if Ctrl is currently pressed
            let ctrl_state = GetKeyState(VK_CONTROL.0 as i32);
            if (ctrl_state & 0x8000u16 as i16) != 0 {
                // Ctrl+C detected! Wait a moment for clipboard to update, then sync
                log::debug_log!("Ctrl+C detected! Syncing clipboard...");
                
                // Access APP_STATE via raw pointer
                let state_ptr = std::ptr::addr_of!(APP_STATE);
                if let Some(state_arc) = (*state_ptr).as_ref() {
                    let state = state_arc.clone();
                    tokio::spawn(async move {
                        // Small delay to let clipboard update
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        if let Err(e) = handle_clipboard_change(state).await {
                            log::debug_error!("Clipboard sync error: {}", e);
                        }
                    });
                }
            }
        }
    }
    
    CallNextHookEx(None, n_code, w_param, l_param)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

async fn handle_clipboard_change(state_arc: Arc<Mutex<AppState>>) -> Result<(), Box<dyn std::error::Error>> {
    let text = get_clipboard_text()?;
    
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
    
    // Check if this is the same text we just received from network
    if let Some(prev_hash) = *last_hash {
        if prev_hash == text_hash {
            log::debug_log!("Ignoring - this is an echo from Android");
            return Ok(()); // Ignore echo
        }
    }
    
    *last_hash = Some(text_hash);
    drop(last_hash);
    
    // Encrypt and broadcast
    let encrypted = state.crypto.encrypt(&text)?;
    network::broadcast_clipboard(&encrypted).await?;
    
    log::debug_log!("Clipboard synced: {} bytes", text.len());
    
    Ok(())
}

fn get_clipboard_text() -> Result<String, Box<dyn std::error::Error>> {
    unsafe {
        OpenClipboard(None)?;

        let result = match IsClipboardFormatAvailable(CF_UNICODETEXT) {
            Ok(_) => {
                match GetClipboardData(CF_UNICODETEXT) {
                    Ok(h_data) => {
                        if h_data.is_invalid() {
                            Ok(String::new())
                        } else {
                            // Cast HANDLE to HGLOBAL
                            let h_global = HGLOBAL(h_data.0);
                            let ptr = GlobalLock(h_global) as *const u16;
                            if !ptr.is_null() {
                                let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                                let slice = std::slice::from_raw_parts(ptr, len);
                                let text = String::from_utf16_lossy(slice);
                                let _ = GlobalUnlock(h_global);
                                Ok(text)
                            } else {
                                Ok(String::new())
                            }
                        }
                    }
                    Err(_) => Ok(String::new())
                }
            }
            Err(_) => Ok(String::new())
        };

        CloseClipboard()?;
        result
    }
}

pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    // Retry mechanism for clipboard operations
    // Windows clipboard can be temporarily unavailable if another app is using it
    let max_attempts = 5;
    let mut last_error = None;
    
    for attempt in 1..=max_attempts {
        match try_set_clipboard(text) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

fn try_set_clipboard(text: &str) -> Result<(), String> {
    unsafe {
        // Try to open clipboard
        OpenClipboard(None).map_err(|e| format!("Failed to open clipboard: {}", e))?;
        
        EmptyClipboard().map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to empty clipboard: {}", e)
        })?;

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;

        let h_mem = GlobalAlloc(GMEM_MOVEABLE, size).map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to allocate memory: {}", e)
        })?;
        
        let ptr = GlobalLock(h_mem) as *mut u16;
        
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err("Failed to lock memory".to_string());
        }
        
        ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        
        // GlobalUnlock returns 0 on success when refcount reaches 0, which is normal
        // We don't check the result because 0 is actually success here
        let _ = GlobalUnlock(h_mem);
        
        // Cast HGLOBAL to HANDLE for SetClipboardData
        SetClipboardData(CF_UNICODETEXT, HANDLE(h_mem.0)).map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to set clipboard data: {}", e)
        })?;

        CloseClipboard().map_err(|e| format!("Failed to close clipboard: {}", e))?;
    }

    Ok(())
}

pub fn hash_text(text: &str) -> u64 {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    u64::from_le_bytes(result[0..8].try_into().unwrap())
}

pub async fn update_last_hash(state: &AppState, hash: u64) {
    let mut last_hash = state.last_clipboard_hash.lock().await;
    *last_hash = Some(hash);
}
