#![allow(dead_code)]

use serde::Serialize;

/// Event payload for VAD level updates
#[derive(Debug, Clone, Serialize)]
pub struct VadLevelEvent {
    pub rms: f32,
}

/// Event payload for transcript updates
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptEvent {
    pub text: String,
}

/// All possible app states
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StateChangeEvent {
    Idle,
    Listening,
    Processing,
    Error,
}

/// Permission types that might be required
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRequiredEvent {
    Microphone,
    Accessibility,
}
