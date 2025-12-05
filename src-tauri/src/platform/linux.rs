//! Linux platform implementation

use super::{PermissionStatus, PlatformIntegration, PlatformResult};
use std::fs;
use std::process::Command;
use tauri::WebviewWindow;

/// Linux platform integration
pub struct LinuxPlatform;

impl PlatformIntegration for LinuxPlatform {
    fn check_microphone_permission(&self) -> PermissionStatus {
        // Linux uses PulseAudio/PipeWire for audio
        // Permissions are typically controlled by:
        // 1. User group membership (audio group - older systems)
        // 2. PipeWire/PulseAudio access (modern systems)
        // 3. File permissions on /dev/snd/* devices

        // Check if audio devices exist and are accessible
        if let Ok(entries) = fs::read_dir("/dev/snd") {
            let has_capture_device = entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                // Look for capture devices (pcmC*D*c where c = capture)
                name_str.starts_with("pcm") && name_str.contains("c")
            });

            if !has_capture_device {
                tracing::warn!("Linux: No audio capture devices found in /dev/snd");
                return PermissionStatus::NotDetermined;
            }

            // Check if we can access at least one capture device
            for entry in fs::read_dir("/dev/snd").ok().into_iter().flatten() {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();

                    if name_str.starts_with("pcm") && name_str.contains("c") {
                        // Try to access the device (read metadata)
                        if path.metadata().is_ok() {
                            tracing::info!(
                                "Linux: Audio capture device accessible: {}",
                                path.display()
                            );
                            return PermissionStatus::Authorized;
                        }
                    }
                }
            }
        } else {
            tracing::warn!("Linux: Cannot access /dev/snd directory");
        }

        // Fallback: Check if user is in audio group (older systems)
        if let Ok(output) = Command::new("id").arg("-Gn").output() {
            if let Ok(groups) = String::from_utf8(output.stdout) {
                if groups.split_whitespace().any(|g| g == "audio") {
                    tracing::info!("Linux: User is in 'audio' group");
                    return PermissionStatus::Authorized;
                }
            }
        }

        // On modern systems with PipeWire, permissions are more dynamic
        // We can't easily check them, so we assume authorized and let cpal fail if needed
        tracing::info!("Linux: Assuming microphone access via PipeWire/PulseAudio");
        PermissionStatus::Authorized
    }

    fn request_microphone_permission(&self) -> PlatformResult<bool> {
        // Linux doesn't have a standard permission dialog like macOS/Windows
        // Check current status first
        let status = self.check_microphone_permission();

        match status {
            PermissionStatus::Authorized => {
                tracing::info!("Linux: Microphone access already available");
                Ok(true)
            }
            _ => {
                // Provide helpful diagnostic information
                tracing::warn!("Linux: Microphone access may be restricted");
                tracing::info!("Linux: Troubleshooting steps:");
                tracing::info!(
                    "  1. Ensure your user is in the 'audio' group: sudo usermod -aG audio $USER"
                );
                tracing::info!(
                    "  2. Check PipeWire/PulseAudio is running: systemctl --user status pipewire"
                );
                tracing::info!("  3. Verify audio devices exist: ls -l /dev/snd/");
                tracing::info!("  4. Test with: arecord -l (list capture devices)");

                // Return true to let the app try - cpal will provide better error messages
                Ok(true)
            }
        }
    }

    fn configure_overlay_window(&self, window: &WebviewWindow) -> PlatformResult<()> {
        // Configure Linux overlay window with X11 hints (via GTK)
        match configure_linux_overlay(window) {
            Ok(_) => {
                tracing::info!("Linux: Overlay window configured successfully");
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Linux: Failed to configure overlay window: {}", e);
                tracing::info!(
                    "       Window behavior may vary depending on window manager (X11/Wayland)"
                );
                // Non-fatal error - window will still work
                Ok(())
            }
        }
    }
}

/// Configure Linux overlay window
/// Works on X11, limited/no support on Wayland
///
/// Note: Full GTK configuration is not yet implemented.
/// The window will use Tauri's default behavior which includes:
/// - decorations: false (from tauri.conf.json)
/// - resizable: false
/// - alwaysOnTop: true (should be set in tauri.conf.json)
#[cfg(target_os = "linux")]
fn configure_linux_overlay(_window: &WebviewWindow) -> Result<(), String> {
    // Detect if running on Wayland
    if is_wayland() {
        tracing::warn!("Linux: Running on Wayland - window behavior may differ from X11");
        tracing::info!("       For consistent overlay behavior, use X11 session");
        tracing::info!("       See WAYLAND_LIMITATIONS.md for details");
    } else {
        tracing::info!("Linux: Running on X11");
    }

    // TODO: Implement GTK window hints when gtk-rs integration is available
    // Would configure: set_keep_above, set_skip_taskbar_hint, set_skip_pager_hint, stick, set_accept_focus
    // For now, rely on Tauri window configuration from tauri.conf.json

    tracing::info!(
        "Linux: Using Tauri window configuration (decorations=false, alwaysOnTop via config)"
    );
    tracing::info!(
        "       Advanced overlay features (skip_taskbar, no_focus) require manual window rules"
    );

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_overlay(_window: &WebviewWindow) -> Result<(), String> {
    Err("Not Linux".to_string())
}

/// Detect if running on Wayland
#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    // Check common Wayland environment variables
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|s| s.to_lowercase().contains("wayland"))
            .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn is_wayland() -> bool {
    false
}
