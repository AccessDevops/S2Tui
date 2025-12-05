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
            .map_err(|_| PlatformError::WindowHandleUnavailable)?;

        let ns_window = ns_window as *mut AnyObject;

        unsafe {
            // Set the window to be a non-activating panel
            // NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorStationary | NSWindowCollectionBehaviorIgnoresCycle
            let behavior: u64 = (1 << 0) | (1 << 4) | (1 << 6);
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];

            // Configure window transparency
            // Make window non-opaque to allow transparency
            let _: () = msg_send![ns_window, setOpaque: Bool::NO];

            // Set window background to clear color
            let clear_color: *mut AnyObject = msg_send![objc2::class!(NSColor), clearColor];
            let _: () = msg_send![ns_window, setBackgroundColor: clear_color];

            // Get content view and configure transparency
            let content_view: *mut AnyObject = msg_send![ns_window, contentView];
            if !content_view.is_null() {
                // Make content view layer-backed with transparent background
                let _: () = msg_send![content_view, setWantsLayer: Bool::YES];

                // Get the layer and set clear background
                let layer: *mut AnyObject = msg_send![content_view, layer];
                if !layer.is_null() {
                    // CGColorGetConstantColor(kCGColorClear) - using nil for clear
                    let _: () = msg_send![layer, setBackgroundColor: std::ptr::null::<AnyObject>()];
                }

                // Recursively configure all subviews for transparency (including WKWebView)
                configure_subviews_transparent(content_view);
            }
        }

        tracing::info!("Window configured as non-focusable overlay with transparency");
        Ok(())
    }
}

/// Recursively configure subviews for transparency
/// This ensures the WKWebView and all intermediate views are transparent
unsafe fn configure_subviews_transparent(view: *mut AnyObject) {
    if view.is_null() {
        return;
    }

    // Get class name to check if it's a WKWebView
    let class: *mut AnyObject = msg_send![view, class];
    let class_name: *mut AnyObject = msg_send![class, className];
    let class_name_str: *const std::ffi::c_char = msg_send![class_name, UTF8String];

    if !class_name_str.is_null() {
        let name = std::ffi::CStr::from_ptr(class_name_str)
            .to_string_lossy()
            .to_string();

        // Configure WKWebView specifically
        if name.contains("WKWebView") {
            // Try to set _drawsBackground to NO (private API but commonly used)
            let _: () = msg_send![view, setValue: Bool::NO, forKey: objc2_foundation::ns_string!("drawsBackground")];
            tracing::debug!("Configured WKWebView transparency");
        }
    }

    // Make view layer-backed
    let _: () = msg_send![view, setWantsLayer: Bool::YES];

    // Get subviews array
    let subviews: *mut AnyObject = msg_send![view, subviews];
    if subviews.is_null() {
        return;
    }

    let count: usize = msg_send![subviews, count];
    for i in 0..count {
        let subview: *mut AnyObject = msg_send![subviews, objectAtIndex: i];
        configure_subviews_transparent(subview);
    }
}
