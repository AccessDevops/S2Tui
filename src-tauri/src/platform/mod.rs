//! Platform-specific functionality abstraction
//!
//! This module provides a trait-based abstraction for platform-specific operations
//! such as permissions checking, window configuration, etc.

pub mod types;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

pub use types::*;

use tauri::WebviewWindow;

/// Platform-specific integration trait
///
/// Implementations provide platform-specific behavior for permissions,
/// window management, and other OS-specific operations.
pub trait PlatformIntegration: Send + Sync {
    /// Check current microphone permission status
    fn check_microphone_permission(&self) -> PermissionStatus;

    /// Request microphone permission from the system
    ///
    /// This may show a system dialog on some platforms.
    /// Returns true if permission was granted.
    fn request_microphone_permission(&self) -> PlatformResult<bool>;

    /// Configure the overlay window with platform-specific settings
    ///
    /// This configures window behaviors like:
    /// - Not stealing focus when clicked
    /// - Staying above other windows
    /// - Hiding from taskbar/dock
    fn configure_overlay_window(&self, window: &WebviewWindow) -> PlatformResult<()>;
}

/// Get the platform integration instance for the current OS
pub fn get_platform() -> Box<dyn PlatformIntegration> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSPlatform)
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPlatform)
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxPlatform)
    }
}

/// Convenience functions for use in commands
pub fn is_microphone_authorized() -> bool {
    get_platform().check_microphone_permission().is_granted()
}

pub fn request_microphone_permission() -> bool {
    get_platform()
        .request_microphone_permission()
        .unwrap_or(false)
}
