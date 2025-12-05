//! Common types used across platform implementations

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Feature not supported on this platform: {0}")]
    NotSupported(String),

    #[error("Platform operation failed: {0}")]
    OperationFailed(String),

    #[error("Window handle not available")]
    WindowHandleUnavailable,
}

pub type PlatformResult<T> = Result<T, PlatformError>;

/// Permission status for microphone access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionStatus {
    /// Permission has been granted
    Authorized,
    /// Permission has been explicitly denied
    Denied,
    /// User has not been asked yet
    NotDetermined,
    /// Permission is restricted (e.g., parental controls)
    Restricted,
}

impl PermissionStatus {
    pub fn is_granted(&self) -> bool {
        matches!(self, PermissionStatus::Authorized)
    }
}
