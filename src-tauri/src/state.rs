use crate::audio::{AudioCapture, VoiceActivityDetector};
use crate::whisper::WhisperWorker;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Idle,
    Listening,
    Processing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Auto,
    En,
    Fr,
    Es,
    De,
    It,
    Pt,
    Nl,
    Ja,
    Zh,
    Ko,
    Ar,
    Hi,
    Pl,
}

impl Language {
    /// ISO 639-1 code expected by whisper-rs. `None` means auto-detect.
    pub fn to_whisper_code(self) -> Option<&'static str> {
        match self {
            Language::Auto => None,
            Language::En => Some("en"),
            Language::Fr => Some("fr"),
            Language::Es => Some("es"),
            Language::De => Some("de"),
            Language::It => Some("it"),
            Language::Pt => Some("pt"),
            Language::Nl => Some("nl"),
            Language::Ja => Some("ja"),
            Language::Zh => Some("zh"),
            Language::Ko => Some("ko"),
            Language::Ar => Some("ar"),
            Language::Hi => Some("hi"),
            Language::Pl => Some("pl"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub model: String,
    pub shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            model: "large-v3-turbo".to_string(),
            shortcut: "CommandOrControl+Shift+Space".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions {
    pub microphone: bool,
}

pub struct AppStateInner {
    pub status: AppStatus,
    pub settings: Settings,
    pub permissions: Permissions,
    pub vu_level: f32,
}

impl Default for AppStateInner {
    fn default() -> Self {
        Self {
            status: AppStatus::Idle,
            settings: Settings::default(),
            permissions: Permissions::default(),
            vu_level: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
    pub audio_capture: Arc<AudioCapture>,
    pub vad: Arc<RwLock<VoiceActivityDetector>>,
    pub whisper: Arc<WhisperWorker>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner::default())),
            audio_capture: Arc::new(AudioCapture::new()),
            vad: Arc::new(RwLock::new(VoiceActivityDetector::new())),
            whisper: Arc::new(WhisperWorker::new()),
        }
    }

    pub fn get_status(&self) -> AppStatus {
        self.inner.read().status
    }

    pub fn set_status(&self, status: AppStatus) {
        self.inner.write().status = status;
    }

    pub fn get_settings(&self) -> Settings {
        self.inner.read().settings.clone()
    }

    pub fn update_settings<F>(&self, f: F)
    where
        F: FnOnce(&mut Settings),
    {
        f(&mut self.inner.write().settings);
    }

    pub fn get_permissions(&self) -> Permissions {
        self.inner.read().permissions.clone()
    }

    pub fn set_permissions(&self, permissions: Permissions) {
        self.inner.write().permissions = permissions;
    }

    pub fn get_vu_level(&self) -> f32 {
        self.inner.read().vu_level
    }

    pub fn set_vu_level(&self, level: f32) {
        self.inner.write().vu_level = level.clamp(0.0, 1.0);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_maps_to_none() {
        assert_eq!(Language::Auto.to_whisper_code(), None);
    }

    #[test]
    fn languages_map_to_iso_codes() {
        assert_eq!(Language::En.to_whisper_code(), Some("en"));
        assert_eq!(Language::Fr.to_whisper_code(), Some("fr"));
        assert_eq!(Language::Ja.to_whisper_code(), Some("ja"));
        assert_eq!(Language::Pl.to_whisper_code(), Some("pl"));
    }
}
