use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::Graphics::Dwm::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Controls::*,
    Win32::UI::WindowsAndMessaging::*,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::log;

// Constants for control styles
const SS_LEFT: u32 = 0x00000000;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_AUTOCHECKBOX: u32 = 0x0003;

// Image constants
const IMAGE_ICON: u32 = 1;

// DWM constants
const DWMWA_USE_IMMERSIVE_DARK_MODE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(20);
const DWMWA_WINDOW_CORNER_PREFERENCE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(33);
const DWMWCP_ROUND: u32 = 2;

// Button styles
const BS_GROUPBOX: u32 = 0x00000007;

// Button messages
const BM_GETCHECK: u32 = 0x00F0;
const BST_CHECKED: LRESULT = LRESULT(1);

// Control IDs
const IDC_STATUS_TEXT: i32 = 1001;
const IDC_AUTOSTART_CHECK: i32 = 1004;
const IDC_CLOSE_BUTTON: i32 = 1006;

fn loword(l: usize) -> u16 {
    (l & 0xffff) as u16
}

pub struct SettingsWindow {
    hwnd: HWND,
    is_open: Arc<AtomicBool>,
}

impl SettingsWindow {
    pub fn new() -> Result<Self> {
        let is_open = Arc::new(AtomicBool::new(false));
        
        unsafe {
            let h_instance = GetModuleHandleW(None)?;
            let class_name = w!("AWClippySettings");
            
            // Use system background brush for proper theme support
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::wnd_proc),
                hInstance: h_instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: GetSysColorBrush(COLOR_WINDOW),
                lpszClassName: class_name,
                ..Default::default()
            };

            RegisterClassW(&wc);

            // Create modern-styled window
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("AW Clippy Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                550,
                340,
                None,
                None,
                h_instance,
                None,
            )?;

            // Apply Windows 11 styling
            Self::apply_windows11_style(hwnd);
            
            // Set window icon
            Self::set_window_icon(hwnd);

            Ok(Self { hwnd, is_open })
        }
    }

    unsafe fn apply_windows11_style(hwnd: HWND) {
        // Enable rounded corners (Windows 11)
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // Check if dark mode is enabled
        if Self::is_dark_mode() {
            let dark_mode: u32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    unsafe fn set_window_icon(hwnd: HWND) {
        // Try to load the icon from the embedded resource or file
        if let Ok(icon) = Self::load_window_icon() {
            SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(icon.0 as isize)); // ICON_SMALL
            SendMessageW(hwnd, WM_SETICON, WPARAM(1), LPARAM(icon.0 as isize)); // ICON_BIG
        }
    }

    unsafe fn load_window_icon() -> Result<HICON> {
        // Try to load ICO from disk first (optional external icon for customization)
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        
        if let Some(dir) = exe_dir {
            let icon_path = dir.join("icons").join("app_icon.ico");
            
            if icon_path.exists() {
                let path_wide: Vec<u16> = icon_path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                
                if let Ok(handle) = LoadImageW(
                    None,
                    PCWSTR(path_wide.as_ptr()),
                    GDI_IMAGE_TYPE(IMAGE_ICON),
                    32,
                    32,
                    LR_LOADFROMFILE,
                ) {
                    log::debug_log!("✓ Loaded window icon from external file");
                    return Ok(HICON(handle.0));
                }
            }
        }
        
        // Try embedded icon as fallback (default)
        if let Ok(icon) = Self::load_embedded_icon() {
            log::debug_log!("✓ Loaded embedded window icon");
            return Ok(icon);
        }
        
        // Last resort: Use application icon from system
        log::debug_log!("⚠ Using default system icon");
        LoadIconW(None, IDI_APPLICATION)
    }
    
    unsafe fn load_embedded_icon() -> Result<HICON> {
        // Embedded ICO file
        const ICO_BYTES: &[u8] = include_bytes!("../icons/app_icon.ico");
        
        // Write to temp file and load (Windows needs a file path for ICO)
        let temp_dir = std::env::temp_dir();
        let temp_icon = temp_dir.join("awclippy_icon.ico");
        
        if std::fs::write(&temp_icon, ICO_BYTES).is_ok() {
            let path_wide: Vec<u16> = temp_icon
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            
            let result = LoadImageW(
                None,
                PCWSTR(path_wide.as_ptr()),
                GDI_IMAGE_TYPE(IMAGE_ICON),
                32,
                32,
                LR_LOADFROMFILE,
            );
            
            // Clean up temp file
            let _ = std::fs::remove_file(&temp_icon);
            
            if let Ok(handle) = result {
                return Ok(HICON(handle.0));
            }
        }
        
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to load embedded icon").into())
    }

    unsafe fn is_dark_mode() -> bool {
        use windows::Win32::System::Registry::*;
        
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            0,
            KEY_READ,
            &mut hkey,
        );

        if result.is_ok() {
            let mut data: u32 = 0;
            let mut data_size = std::mem::size_of::<u32>() as u32;
            let result = RegQueryValueExW(
                hkey,
                w!("AppsUseLightTheme"),
                None,
                None,
                Some(&mut data as *mut _ as *mut _),
                Some(&mut data_size),
            );
            let _ = RegCloseKey(hkey);
            
            if result.is_ok() {
                return data == 0; // 0 = dark mode
            }
        }
        false
    }

    pub fn show(&self, network_name: &str, device_name: Option<&str>) {
        let status_str = if let Some(phone) = device_name {
            format!("Connected to {}, but synchronized with {}", network_name, phone)
        } else {
            format!("Connected to {} (Not synchronized)", network_name)
        };
        let status_wide: Vec<u16> = status_str.encode_utf16().chain(Some(0)).collect();

        if self.is_open.swap(true, Ordering::SeqCst) {
            unsafe {
                if let Ok(ctrl) = GetDlgItem(self.hwnd, IDC_STATUS_TEXT) {
                    if !ctrl.is_invalid() {
                        let _ = SetWindowTextW(ctrl, PCWSTR(status_wide.as_ptr()));
                    }
                }
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(self.hwnd);
            }
            return;
        }

        unsafe {
            SetWindowTheme(self.hwnd, w!("Explorer"), PCWSTR::null()).ok();
            self.create_controls(&status_str);
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = UpdateWindow(self.hwnd);
        }
    }

    unsafe fn create_controls(&self, initial_status: &str) {
        let h_instance = GetModuleHandleW(None).unwrap();
        let _is_dark = Self::is_dark_mode();
        
        // Modern Segoe UI Variable font (Windows 11)
        let font = CreateFontW(
            -14, 0, 0, 0,
            FW_NORMAL.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (FF_DONTCARE.0 | DEFAULT_PITCH.0) as u32,
            w!("Segoe UI Variable Display"),
        );

        // Header section with icon and title
        let header_y = 20;
        
        // Title with larger font
        let title_font = CreateFontW(
            -20, 0, 0, 0,
            FW_SEMIBOLD.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (FF_DONTCARE.0 | DEFAULT_PITCH.0) as u32,
            w!("Segoe UI Variable Display"),
        );

        let title = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Settings"),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(SS_LEFT),
            30, header_y, 400, 32,
            self.hwnd, None, h_instance, None,
        ).unwrap();
        SendMessageW(title, WM_SETFONT, WPARAM(title_font.0 as usize), LPARAM(1));

        // Status group
        let group_y = header_y + 60;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("Connection Status"),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(BS_GROUPBOX),
            20, group_y, 510, 60,
            self.hwnd, None, h_instance, None,
        ).unwrap();

        // Status icon (using system icons) - simplified without icon for now
        let status_label = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Status:"),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(SS_LEFT),
            40, group_y + 25, 80, 20,
            self.hwnd, None, h_instance, None,
        ).unwrap();
        SendMessageW(status_label, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

        let status_wide: Vec<u16> = initial_status.encode_utf16().chain(Some(0)).collect();
        let status_value = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR(status_wide.as_ptr()),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(SS_LEFT),
            130, group_y + 25, 360, 20,
            self.hwnd, HMENU(IDC_STATUS_TEXT as _), h_instance, None,
        ).unwrap();
        SendMessageW(status_value, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

        // Configuration group
        let config_y = group_y + 80;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("Startup"),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(BS_GROUPBOX),
            20, config_y, 510, 60,
            self.hwnd, None, h_instance, None,
        ).unwrap();

        // Auto-start checkbox
        let autostart = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("Start with Windows"),
            WS_VISIBLE | WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX),
            40, config_y + 25, 200, 25,
            self.hwnd, HMENU(IDC_AUTOSTART_CHECK as _), h_instance, None,
        ).unwrap();
        SendMessageW(autostart, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

        // Check current autostart status
        if Self::is_autostart_enabled() {
            SendMessageW(autostart, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
        }

        // Info text
        let info_y = config_y + 80;
        let info = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Note: Configuration settings are compiled into the application.\nTo change ports or shared secret, you must rebuild both Windows and Android apps."),
            WS_VISIBLE | WS_CHILD | WINDOW_STYLE(SS_LEFT),
            30, info_y, 490, 50,
            self.hwnd, None, h_instance, None,
        ).unwrap();
        SendMessageW(info, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));

        // Action buttons (modern Windows 11 style with accent color)
        let button_y = info_y + 70;
        let close_btn = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("Close"),
            WS_VISIBLE | WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON) | WINDOW_STYLE(0x00000001), // BS_DEFPUSHBUTTON
            390, button_y, 140, 36,
            self.hwnd, HMENU(IDC_CLOSE_BUTTON as _), h_instance, None,
        ).unwrap();
        SendMessageW(close_btn, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        SetWindowTheme(close_btn, w!("Explorer"), PCWSTR::null()).ok();
    }

    unsafe fn is_autostart_enabled() -> bool {
        use windows::Win32::System::Registry::*;
        
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_READ,
            &mut hkey,
        );

        if result.is_ok() {
            let mut buffer = [0u16; 512];
            let mut buffer_size = (buffer.len() * 2) as u32;
            let result = RegQueryValueExW(
                hkey,
                w!("AWClippy"),
                None,
                None,
                Some(buffer.as_mut_ptr() as *mut _),
                Some(&mut buffer_size),
            );
            let _ = RegCloseKey(hkey);
            
            return result.is_ok();
        }
        false
    }

    unsafe fn set_autostart(enable: bool) -> Result<()> {
        use windows::Win32::System::Registry::*;
        
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_WRITE,
            &mut hkey,
        );
        
        if result.is_err() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to open registry key").into());
        }

        if enable {
            // Get current executable path
            let exe_path = std::env::current_exe()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            
            let path_wide: Vec<u16> = exe_path.encode_utf16().chain(Some(0)).collect();
            let path_bytes: &[u8] = std::slice::from_raw_parts(
                path_wide.as_ptr() as *const u8,
                path_wide.len() * 2
            );
            
            let result = RegSetValueExW(
                hkey,
                w!("AWClippy"),
                0,
                REG_SZ,
                Some(path_bytes),
            );
            
            if result.is_err() {
                let _ = RegCloseKey(hkey);
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to set registry value").into());
            }
        } else {
            // Remove registry key
            let _ = RegDeleteValueW(hkey, w!("AWClippy"));
        }
        
        let _ = RegCloseKey(hkey);
        Ok(())
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_COMMAND => {
                let control_id = loword(wparam.0) as i32;
                
                match control_id {
                    IDC_AUTOSTART_CHECK => {
                        // Get checkbox state
                        let checkbox = GetDlgItem(hwnd, IDC_AUTOSTART_CHECK).unwrap_or(HWND(std::ptr::null_mut()));
                        if !checkbox.0.is_null() {
                            let checked = SendMessageW(checkbox, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize;
                            
                            // Update registry
                            if let Err(_) = Self::set_autostart(checked) {
                                let _ = MessageBoxW(
                                    hwnd,
                                    w!("Failed to update startup settings."),
                                    w!("Error"),
                                    MB_OK | MB_ICONERROR,
                                );
                            }
                        }
                    }
                    IDC_CLOSE_BUTTON => {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
