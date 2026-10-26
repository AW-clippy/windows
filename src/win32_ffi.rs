//! Windows API FFI Layer
//! 
//! This module isolates all unsafe Windows API calls behind safe abstractions.
//! All `unsafe` code is contained here with careful documentation of safety invariants.

use std::ptr;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HANDLE, HGLOBAL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::DataExchange::{
    GetClipboardData, OpenClipboard, CloseClipboard, 
    IsClipboardFormatAvailable, SetClipboardData, EmptyClipboard,
    AddClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
};
use windows::core::w;

const CF_UNICODETEXT: u32 = 13;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;

/// Type alias for the clipboard change callback
pub type ClipboardCallback = Box<dyn Fn() + Send + Sync + 'static>;

/// Thread-safe global state for clipboard monitoring
/// Using Arc<Mutex<>> instead of static mut for proper Rust safety
static CALLBACK_STORE: Mutex<Option<Arc<ClipboardCallback>>> = Mutex::new(None);

/// Safe wrapper around Windows clipboard monitoring
pub struct ClipboardMonitor {
    hwnd: HWND,
}

// SAFETY: HWND is just an opaque handle that Windows uses for window identification.
// It's safe to send between threads (the OS manages the actual window state).
// Windows message handling is thread-safe at the OS level.
unsafe impl Send for ClipboardMonitor {}
unsafe impl Sync for ClipboardMonitor {}

impl ClipboardMonitor {
    /// Create a new clipboard monitor with a callback function
    /// 
    /// # Safety
    /// The callback will be invoked from the Windows message loop thread.
    /// It should be thread-safe and non-blocking.
    pub fn new<F>(callback: F) -> Result<Self, String>
    where
        F: Fn() + Send + Sync + 'static,
    {
        // Store callback in thread-safe global
        {
            let mut store = CALLBACK_STORE.lock().unwrap();
            *store = Some(Arc::new(Box::new(callback)));
        }

        unsafe {
            let h_instance = GetModuleHandleW(None)
                .map_err(|e| format!("Failed to get module handle: {}", e))?;
            
            let class_name = w!("AWClippyMonitorClass");
            
            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: h_instance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };

            if RegisterClassW(&wnd_class) == 0 {
                return Err("Failed to register window class".to_string());
            }

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("AWClippy Monitor"),
                WINDOW_STYLE::default(),
                0, 0, 0, 0,
                None,
                None,
                h_instance,
                None,
            ).map_err(|e| format!("Failed to create window: {}", e))?;

            // Register clipboard listener
            AddClipboardFormatListener(hwnd)
                .map_err(|e| format!("Failed to add clipboard listener: {}", e))?;

            Ok(ClipboardMonitor { hwnd })
        }
    }

    /// Get the window handle for this monitor
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

/// Windows message procedure callback
/// 
/// # Safety
/// This is called by the Windows OS from arbitrary threads.
/// We minimize unsafe operations and delegate to safe Rust code quickly.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        // Invoke the stored callback in a thread-safe manner
        if let Ok(store) = CALLBACK_STORE.lock() {
            if let Some(callback) = store.as_ref() {
                callback();
            }
        }
        return LRESULT(0);
    }

    // Handle Windows shutdown messages
    if msg == 0x0010 /* WM_CLOSE */ 
        || msg == 0x0002 /* WM_DESTROY */ 
        || msg == 0x0012 /* WM_QUIT */ 
        || msg == 0x0011 /* WM_QUERYENDSESSION */ 
        || msg == 0x0016 /* WM_ENDSESSION */ 
    {
        std::process::exit(0);
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Read text from the Windows clipboard
pub fn read_clipboard() -> Result<String, String> {
    unsafe {
        OpenClipboard(None)
            .map_err(|e| format!("Failed to open clipboard: {}", e))?;

        let result = match IsClipboardFormatAvailable(CF_UNICODETEXT) {
            Ok(_) => {
                match GetClipboardData(CF_UNICODETEXT) {
                    Ok(h_data) => {
                        if h_data.is_invalid() {
                            Ok(String::new())
                        } else {
                            let h_global = HGLOBAL(h_data.0);
                            let ptr = GlobalLock(h_global) as *const u16;
                            
                            if ptr.is_null() {
                                Ok(String::new())
                            } else {
                                // Find null terminator
                                let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                                let slice = std::slice::from_raw_parts(ptr, len);
                                let text = String::from_utf16_lossy(slice);
                                
                                // Always unlock before returning
                                let _ = GlobalUnlock(h_global);
                                Ok(text)
                            }
                        }
                    }
                    Err(_) => Ok(String::new())
                }
            }
            Err(_) => Ok(String::new())
        };

        // Always close clipboard
        let _ = CloseClipboard();
        result
    }
}

/// Write text to the Windows clipboard
/// 
/// Retries up to 5 times with 50ms delay between attempts to handle
/// cases where another application has the clipboard open.
pub fn write_clipboard(text: &str) -> Result<(), String> {
    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY_MS: u64 = 50;
    
    let mut last_error = None;
    
    for attempt in 1..=MAX_ATTEMPTS {
        match try_write_clipboard(text) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

/// Single attempt to write to clipboard
/// 
/// # Safety
/// Uses Windows clipboard APIs which require careful memory management.
/// We ensure proper cleanup even on error paths.
fn try_write_clipboard(text: &str) -> Result<(), String> {
    unsafe {
        OpenClipboard(None)
            .map_err(|e| format!("Failed to open clipboard: {}", e))?;
        
        // Empty clipboard first
        EmptyClipboard().map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to empty clipboard: {}", e)
        })?;

        // Convert text to UTF-16 with null terminator
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;

        // Allocate global memory
        let h_mem = GlobalAlloc(GMEM_MOVEABLE, size).map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to allocate memory: {}", e)
        })?;
        
        // Lock memory and copy data
        let ptr = GlobalLock(h_mem) as *mut u16;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err("Failed to lock memory".to_string());
        }
        
        ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        
        // Always unlock before setting clipboard
        let _ = GlobalUnlock(h_mem);
        
        // Set clipboard data
        SetClipboardData(CF_UNICODETEXT, HANDLE(h_mem.0)).map_err(|e| {
            let _ = CloseClipboard();
            format!("Failed to set clipboard data: {}", e)
        })?;

        // Always close clipboard
        CloseClipboard()
            .map_err(|e| format!("Failed to close clipboard: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_round_trip() {
        let test_text = "Hello, AWClippy!";
        
        // Write
        write_clipboard(test_text).expect("Failed to write clipboard");
        
        // Small delay to ensure Windows processes the change
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        // Read
        let read_text = read_clipboard().expect("Failed to read clipboard");
        
        assert_eq!(test_text, read_text);
    }
}
