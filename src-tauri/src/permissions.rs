//! macOS permission handling for microphone access

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use std::sync::mpsc;

    /// Check current microphone authorization status
    pub fn check_microphone_status() -> AVAuthorizationStatus {
        unsafe {
            let media_type = AVMediaTypeAudio.expect("AVMediaTypeAudio should be available");
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        }
    }

    /// Request microphone permission from the system
    /// This will trigger the native macOS permission dialog if not yet determined
    /// Returns true if permission was granted, false otherwise
    pub fn request_microphone_permission() -> bool {
        // Check if already authorized
        let status = check_microphone_status();
        if status == AVAuthorizationStatus::Authorized {
            return true;
        }

        // If denied or restricted, we can't request again
        if status == AVAuthorizationStatus::Denied || status == AVAuthorizationStatus::Restricted {
            return false;
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
        rx.recv().unwrap_or(false)
    }

    /// Check if microphone permission is currently granted
    pub fn is_microphone_authorized() -> bool {
        check_microphone_status() == AVAuthorizationStatus::Authorized
    }
}

#[cfg(target_os = "macos")]
pub use macos::*;

// Fallback for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub fn request_microphone_permission() -> bool {
    true // Other platforms don't need explicit permission via this API
}

#[cfg(not(target_os = "macos"))]
pub fn is_microphone_authorized() -> bool {
    true
}
