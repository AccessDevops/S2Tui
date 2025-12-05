//! macOS platform implementation

use super::{PermissionStatus, PlatformError, PlatformIntegration, PlatformResult};
use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyObject, Bool};
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use std::sync::mpsc;
use tauri::WebviewWindow;

/// macOS platform integration
pub struct MacOSPlatform;

impl PlatformIntegration for MacOSPlatform {
    fn check_microphone_permission(&self) -> PermissionStatus {
        let status = unsafe {
            let media_type = AVMediaTypeAudio.expect("AVMediaTypeAudio should be available");
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        };

        match status {
            AVAuthorizationStatus::Authorized => PermissionStatus::Authorized,
            AVAuthorizationStatus::Denied => PermissionStatus::Denied,
            AVAuthorizationStatus::NotDetermined => PermissionStatus::NotDetermined,
            AVAuthorizationStatus::Restricted => PermissionStatus::Restricted,
            _ => PermissionStatus::NotDetermined,
        }
    }

    fn request_microphone_permission(&self) -> PlatformResult<bool> {
        // Check if already authorized
        let status = self.check_microphone_permission();
        if status == PermissionStatus::Authorized {
            return Ok(true);
        }

        // If denied or restricted, we can't request again
        if status == PermissionStatus::Denied || status == PermissionStatus::Restricted {
            return Ok(false);
        }

        // Status is NotDetermined - request permission
        let (tx, rx) = mpsc::channel();

        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });

        unsafe {
            let media_type = AVMediaTypeAudio.expect("AVMediaTypeAudio should be available");
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block);
        }

        // Wait for the user's response (blocking)
        rx.recv().map_err(|e| {
            PlatformError::OperationFailed(format!("Permission request failed: {}", e))
        })
    }

    fn configure_overlay_window(&self, window: &WebviewWindow) -> PlatformResult<()> {
        // Get the NSWindow handle
        let ns_window = window
            .ns_window()
            .map_err(|e| PlatformError::WindowHandleUnavailable)?;

        let ns_window = ns_window as *mut AnyObject;

        unsafe {
            // Set the window to be a non-activating panel
            // NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorStationary | NSWindowCollectionBehaviorIgnoresCycle
            let behavior: u64 = (1 << 0) | (1 << 4) | (1 << 6);
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        }

        tracing::info!("Window configured as non-focusable overlay");
        Ok(())
    }
}
