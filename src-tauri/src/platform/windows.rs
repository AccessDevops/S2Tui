//! Windows platform implementation

use super::{PermissionStatus, PlatformIntegration, PlatformResult};
use tauri::WebviewWindow;

/// Windows platform integration
pub struct WindowsPlatform;

impl PlatformIntegration for WindowsPlatform {
    fn check_microphone_permission(&self) -> PermissionStatus {
        // Windows 10+ has microphone permissions in Settings > Privacy & security > Microphone
        // We can't directly query Windows privacy settings without using Windows APIs,
        // but we can try to enumerate audio devices with cpal as a proxy check.

        // Try to enumerate audio input devices
        match check_audio_devices_available() {
            Ok(true) => {
                tracing::info!("Windows: Audio input devices are accessible");
                PermissionStatus::Authorized
            }
            Ok(false) => {
                tracing::warn!("Windows: No audio input devices found");
                PermissionStatus::NotDetermined
            }
            Err(e) => {
                tracing::error!("Windows: Failed to check audio devices: {}", e);
                // If we can't check, assume restricted and provide guidance
                PermissionStatus::NotDetermined
            }
        }
    }

    fn request_microphone_permission(&self) -> PlatformResult<bool> {
        // Windows doesn't have a programmatic API to request microphone permission like macOS
        // The user must manually enable it in Settings

        let status = self.check_microphone_permission();

        match status {
            PermissionStatus::Authorized => {
                tracing::info!("Windows: Microphone access already available");
                Ok(true)
            }
            _ => {
                tracing::warn!("Windows: Microphone access may be restricted");
                tracing::info!("Windows: Please check the following:");
                tracing::info!("  1. Open Settings > Privacy & security > Microphone");
                tracing::info!(
                    "  2. Enable 'Microphone access' and 'Let apps access your microphone'"
                );
                tracing::info!("  3. Ensure S2Tui is in the allowed apps list");

                // Try to open Windows Settings to microphone privacy page
                if let Err(e) = open_windows_microphone_settings() {
                    tracing::warn!("Failed to open Windows Settings: {}", e);
                }

                // Return true to let the app try - cpal will provide better error messages
                Ok(true)
            }
        }
    }

    fn configure_overlay_window(&self, window: &WebviewWindow) -> PlatformResult<()> {
        // Configure Windows overlay with extended styles
        match configure_windows_overlay(window) {
            Ok(_) => {
                tracing::info!("Windows: Overlay window configured successfully");
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Windows: Failed to configure overlay window: {}", e);
                // Non-fatal error - window will still work, just without optimal behavior
                Ok(())
            }
        }
    }
}

/// Configure Windows overlay window with extended styles
/// Uses raw HWND manipulation via raw-window-handle
#[cfg(target_os = "windows")]
fn configure_windows_overlay(window: &WebviewWindow) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // Get the window handle
    let window_handle = window
        .window_handle()
        .map_err(|e| format!("Failed to get window handle: {}", e))?;

    let hwnd = match window_handle.as_ref() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get(),
        _ => return Err("Not a Win32 window".to_string()),
    };

    unsafe {
        // Win32 API constants
        const GWL_EXSTYLE: i32 = -20;
        const WS_EX_TOOLWINDOW: u32 = 0x00000080; // Tool window, hidden from Alt+Tab
        const WS_EX_TOPMOST: u32 = 0x00000008; // Always on top
        const WS_EX_NOACTIVATE: u32 = 0x08000000; // Don't activate when clicked
        const WS_EX_LAYERED: u32 = 0x00080000; // Layered window (for transparency)
        const LWA_ALPHA: u32 = 0x00000002; // Use bAlpha for transparency

        // External Win32 functions
        #[link(name = "user32")]
        extern "system" {
            fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
            fn SetWindowLongW(hwnd: isize, index: i32, new_long: i32) -> i32;
            fn SetLayeredWindowAttributes(hwnd: isize, crKey: u32, bAlpha: u8, dwFlags: u32)
                -> i32;
        }

        // Get current extended styles
        let current_ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // If WS_EX_LAYERED is present, configure it to receive mouse events
        if current_ex_style & WS_EX_LAYERED != 0 {
            // Set layered attributes: 255 = fully opaque, LWA_ALPHA = use alpha channel
            // This is REQUIRED for WS_EX_LAYERED windows to receive mouse events
            let lwa_result = SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            if lwa_result == 0 {
                tracing::warn!("Windows: SetLayeredWindowAttributes failed");
            }
        }

        // Remove WS_EX_NOACTIVATE (which can block events), add TOOLWINDOW and TOPMOST
        let new_ex_style =
            (current_ex_style & !WS_EX_NOACTIVATE) | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;

        // Apply new extended styles
        let result = SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style as i32);
        if result == 0 {
            tracing::warn!("Windows: SetWindowLongW returned 0, style may not have changed");
        }

        tracing::info!("Windows: Extended styles configured (TOOLWINDOW | TOPMOST)");
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_overlay(_window: &WebviewWindow) -> Result<(), String> {
    Err("Not Windows".to_string())
}

/// Check if audio input devices are available using cpal
fn check_audio_devices_available() -> Result<bool, String> {
    use cpal::traits::HostTrait;

    let host = cpal::default_host();

    // Try to get input devices
    match host.input_devices() {
        Ok(devices) => {
            let count = devices.count();
            tracing::debug!("Windows: Found {} input device(s)", count);
            Ok(count > 0)
        }
        Err(e) => {
            tracing::error!("Windows: Failed to enumerate input devices: {}", e);
            Err(format!("Failed to enumerate audio devices: {}", e))
        }
    }
}

/// Open Windows Settings to the microphone privacy page
/// Uses the ms-settings: URI scheme available in Windows 10+
fn open_windows_microphone_settings() -> Result<(), String> {
    use std::process::Command;

    // Windows 10/11: ms-settings:privacy-microphone opens the microphone privacy page
    match Command::new("cmd")
        .args(["/C", "start", "ms-settings:privacy-microphone"])
        .spawn()
    {
        Ok(_) => {
            tracing::info!("Windows: Opened microphone settings page");
            Ok(())
        }
        Err(e) => Err(format!("Failed to open settings: {}", e)),
    }
}
