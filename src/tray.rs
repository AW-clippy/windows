use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, Icon, TrayIcon,
};
use crate::log;

pub struct TrayMenu {
    tray: TrayIcon,
    pub settings_requested: Arc<AtomicBool>,
    _paused: Arc<AtomicBool>,
    status_item: MenuItem,
}

impl TrayMenu {
    pub fn new(paused: Arc<AtomicBool>) -> Result<Self, Box<dyn std::error::Error>> {
        // Create a simple icon programmatically (16x16 blue square with white center)
        let icon = create_simple_icon()?;

        // Create menu
        let menu = Menu::new();
        
        // Status label (will be updated dynamically)
        let status_item = MenuItem::new("AW Clippy - Searching for Android...", false, None);
        menu.append(&status_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        
        // Settings menu item
        let settings_item = MenuItem::new("Settings", true, None);
        menu.append(&settings_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        
        // Pause/Resume toggle
        let pause_item = MenuItem::new("Pause Clipboard Sync", true, None);
        menu.append(&pause_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        
        // Getting Started submenu
        let _getting_started_label = MenuItem::new("Getting Started", false, None);
        menu.append(&_getting_started_label)?;
        
        let download_item = MenuItem::new("  → Download Android App", true, None);
        menu.append(&download_item)?;
        
        let setup_item = MenuItem::new("  → Setup Instructions", true, None);
        menu.append(&setup_item)?;
        
        menu.append(&PredefinedMenuItem::separator())?;
        
        // Help & Info
        let about_item = MenuItem::new("About AW Clippy", true, None);
        menu.append(&about_item)?;
        
        let help_item = MenuItem::new("Help & Documentation", true, None);
        menu.append(&help_item)?;
        
        let issue_item = MenuItem::new("Report an Issue", true, None);
        menu.append(&issue_item)?;
        
        menu.append(&PredefinedMenuItem::separator())?;
        
        // Exit
        let quit_item = MenuItem::new("Exit AW Clippy", true, None);
        menu.append(&quit_item)?;

        // Build tray icon
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("AW Clippy - Clipboard Sync")
            .with_icon(icon)
            .build()?;

        let settings_requested = Arc::new(AtomicBool::new(false));
        
        // Handle menu events
        let paused_clone = Arc::clone(&paused);
        let settings_clone = Arc::clone(&settings_requested);
        let settings_id = settings_item.id().clone();
        let pause_id = pause_item.id().clone();
        let download_id = download_item.id().clone();
        let setup_id = setup_item.id().clone();
        let about_id = about_item.id().clone();
        let help_id = help_item.id().clone();
        let issue_id = issue_item.id().clone();
        let quit_id = quit_item.id().clone();
        
        std::thread::spawn(move || {
            let menu_channel = MenuEvent::receiver();
            loop {
                if let Ok(event) = menu_channel.recv() {
                    if event.id == settings_id {
                        settings_clone.store(true, Ordering::SeqCst);
                    } else if event.id == pause_id {
                        let was_paused = paused_clone.fetch_xor(true, Ordering::SeqCst);
                        if was_paused {
                            log::debug_log!("✅ Clipboard sync resumed");
                        } else {
                            log::debug_log!("⏸️  Clipboard sync paused");
                        }
                    } else if event.id == download_id {
                        let _ = open::that("https://github.com/YOUR_USERNAME/AWClippy/releases");
                    } else if event.id == setup_id {
                        show_setup_instructions();
                    } else if event.id == about_id {
                        show_about_dialog();
                    } else if event.id == help_id {
                        let _ = open::that("https://github.com/YOUR_USERNAME/AWClippy#readme");
                    } else if event.id == issue_id {
                        let _ = open::that("https://github.com/YOUR_USERNAME/AWClippy/issues");
                    } else if event.id == quit_id {
                        log::debug_log!("Exiting AW Clippy...");
                        std::process::exit(0);
                    }
                }
            }
        });

        Ok(Self {
            tray,
            settings_requested,
            _paused: paused,
            status_item,
        })
    }
    
    pub fn update_status(&self, device_name: Option<&str>) {
        let status_text = if let Some(name) = device_name {
            format!("AW Clippy - Connected to {}", name)
        } else {
            "AW Clippy - Searching for Android...".to_string()
        };
        
        self.status_item.set_text(status_text);
        
        // Update tooltip as well
        let tooltip = if let Some(name) = device_name {
            format!("AW Clippy - Connected to {}", name)
        } else {
            "AW Clippy - Searching for Android device...".to_string()
        };
        
        let _ = self.tray.set_tooltip(Some(tooltip));
    }
}

fn create_simple_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    // Try to load from disk first (optional external icon for customization)
    if let Ok(icon) = load_icon_from_disk() {
        log::debug_log!("✓ Loaded tray icon from external file");
        return Ok(icon);
    }
    
    // Try embedded icon (default)
    if let Ok(icon) = load_embedded_icon() {
        log::debug_log!("✓ Loaded embedded tray icon");
        return Ok(icon);
    }
    
    // Fallback: Create programmatic icon
    log::debug_log!("⚠ Using fallback programmatic icon");
    create_fallback_icon()
}

fn load_icon_from_disk() -> Result<Icon, Box<dyn std::error::Error>> {
    use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_LOADFROMFILE};
    use windows::core::PCWSTR;
    
    // Try to load from icons directory (optional external icon)
    let icon_path = std::env::current_exe()?
        .parent()
        .ok_or("Failed to get exe directory")?
        .join("icons")
        .join("tray_icon.ico");
    
    if !icon_path.exists() {
        return Err("External icon file not available, using embedded icon".into());
    }
    
    let path_wide: Vec<u16> = icon_path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    
    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(path_wide.as_ptr()),
            IMAGE_ICON,
            32,
            32,
            LR_LOADFROMFILE,
        )?;
        
        // Convert HICON to RGBA data for tray-icon crate
        icon_from_hicon(handle.0 as isize)
    }
}

fn load_embedded_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    // Embedded ICO file at compile time
    const ICO_BYTES: &[u8] = include_bytes!("../icons/tray_icon.ico");
    
    // Write to temp file and load (tray-icon needs RGBA, easiest way is through Windows API)
    use std::io::Write;
    let temp_path = std::env::temp_dir().join("awclippy_tray_icon.ico");
    let mut file = std::fs::File::create(&temp_path)?;
    file.write_all(ICO_BYTES)?;
    file.flush()?;
    drop(file);
    
    use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_LOADFROMFILE};
    use windows::core::PCWSTR;
    
    let path_wide: Vec<u16> = temp_path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    
    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(path_wide.as_ptr()),
            IMAGE_ICON,
            32,
            32,
            LR_LOADFROMFILE,
        )?;
        
        icon_from_hicon(handle.0 as isize)
    }
}

fn icon_from_hicon(hicon: isize) -> Result<Icon, Box<dyn std::error::Error>> {
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};
    use windows::Win32::Graphics::Gdi::{
        GetObjectW, DeleteObject, DeleteDC, GetDIBits, CreateCompatibleDC, SelectObject,
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS
    };
    
    unsafe {
        let mut icon_info = ICONINFO::default();
        GetIconInfo(windows::Win32::UI::WindowsAndMessaging::HICON(hicon as *mut _), &mut icon_info)?;
        
        let mut bitmap = BITMAP::default();
        GetObjectW(
            icon_info.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );
        
        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;
        
        // Create a compatible DC and get the bitmap bits
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            // Clean up before returning error
            if !icon_info.hbmColor.is_invalid() {
                let _ = DeleteObject(icon_info.hbmColor);
            }
            if !icon_info.hbmMask.is_invalid() {
                let _ = DeleteObject(icon_info.hbmMask);
            }
            return Err("Failed to create DC".into());
        }
        
        // Select the bitmap into the DC
        let old_bitmap = SelectObject(hdc, icon_info.hbmColor);
        
        // Prepare BITMAPINFO structure
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Negative for top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default(); 1],
        };
        
        // Allocate buffer for BGRA data
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        
        // Get the bitmap bits
        let scan_lines = GetDIBits(
            hdc,
            icon_info.hbmColor,
            0,
            height,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        
        // Restore old bitmap and delete DC
        SelectObject(hdc, old_bitmap);
        let _ = DeleteDC(hdc);
        
        // Clean up icon resources
        if !icon_info.hbmColor.is_invalid() {
            let _ = DeleteObject(icon_info.hbmColor);
        }
        if !icon_info.hbmMask.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask);
        }
        
        if scan_lines == 0 {
            return Err("Failed to get bitmap bits".into());
        }
        
        // Convert BGRA to RGBA
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for i in 0..(width * height) as usize {
            let idx = i * 4;
            rgba[idx] = bgra[idx + 2];     // R
            rgba[idx + 1] = bgra[idx + 1]; // G
            rgba[idx + 2] = bgra[idx];     // B
            rgba[idx + 3] = bgra[idx + 3]; // A
        }
        
        Icon::from_rgba(rgba, width, height).map_err(|e| e.into())
    }
}

fn create_fallback_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    // Create a 32x32 blue square with white "AW" text (minimal, no external deps)
    let size = 32;
    let mut rgba = vec![0u8; size * size * 4];
    
    // Fill with blue background
    for i in 0..size * size {
        rgba[i * 4 + 0] = 70;  // R
        rgba[i * 4 + 1] = 130; // G
        rgba[i * 4 + 2] = 180; // B
        rgba[i * 4 + 3] = 255; // A
    }
    
    Icon::from_rgba(rgba, size as u32, size as u32).map_err(|e| e.into())
}

fn show_setup_instructions() {
    let message = r#"📱 Quick Setup Guide

1️⃣  DOWNLOAD THE ANDROID APP
   Download AW Clippy from GitHub releases or Google Play

2️⃣  INSTALL ON BOTH DEVICES
   • Windows: Keep this app running (it's already active!)
   • Android: Install and open the app

3️⃣  ENABLE ACCESSIBILITY (Android)
   • Open AW Clippy on Android
   • Tap "Enable Accessibility"
   • Find and enable AW Clippy

4️⃣  START THE SERVICE (Android)
   • Tap "Start Service" in the Android app
   • Both devices should auto-discover each other

5️⃣  CONFIGURE SHARED SECRET (Optional)
   • For security, set the same secret in:
     - Windows: Edit .env file (AWCLIPPY_SHARED_SECRET)
     - Android: Edit .env file before building
   • Restart both apps after changing

✅ That's it! Copy on one device, paste on the other!

🔧 Troubleshooting:
   • Make sure both devices are on the same WiFi network
   • Check Windows Firewall isn't blocking AW Clippy
   • Verify ports 48899 and 48900 are available

For detailed help, visit: github.com/YOUR_USERNAME/AWClippy
"#;

    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONINFORMATION};
        
        let title: Vec<u16> = "AW Clippy Setup Guide\0".encode_utf16().collect();
        let msg: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(msg.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

fn show_about_dialog() {
    let message = r#"AW Clippy v0.1.0

🔄 Cross-Platform Clipboard Sync
   Seamlessly sync clipboard between Windows and Android

🔐 Encrypted & Secure
   End-to-end encrypted using ChaCha20-Poly1305

📡 Network Discovery
   Automatic device discovery via mDNS/DNS-SD

💡 Features:
   • Real-time clipboard synchronization
   • Zero-configuration setup
   • Lightweight and fast
   • Open source

👨‍💻 Developed with ❤️
   github.com/YOUR_USERNAME/AWClippy

📄 License: MIT
"#;

    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONINFORMATION};
        
        let title: Vec<u16> = "About AW Clippy\0".encode_utf16().collect();
        let msg: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(msg.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}
