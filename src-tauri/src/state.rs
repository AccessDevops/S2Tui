use crate::audio::{AudioCapture, VoiceActivityDetector};
use crate::whisper::WhisperWorker;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Idle,
    Listening,
    Processing,
    Error,
}

/// All language codes Whisper actually understands. Kept as a static array
/// so `is_known` is a cheap linear scan; ~99 entries is negligible. The
/// frontend `src/utils/languages.ts` registry is the authoritative UI list
/// (display names, flags, tier) — this set merely guards the boundary so
/// junk codes never reach the Whisper engine. Keep both in sync when
/// adding a language.
pub const KNOWN_WHISPER_LANGUAGES: &[&str] = &[
    "auto", // app-level sentinel for auto-detect
    // High & medium tier (currently exposed in the UI)
    "ar", "az", "be", "bg", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es", "et", "eu", "fa",
    "fi", "fr", "gl", "gu", "he", "hi", "hr", "hu", "hy", "id", "is", "it", "ja", "ka", "kk", "km",
    "ko", "lo", "lt", "lv", "mk", "ml", "mn", "mr", "ms", "mt", "my", "ne", "nl", "no", "pl", "pt",
    "ro", "ru", "sk", "sl", "sq", "sr", "sv", "sw", "ta", "te", "th", "tr", "uk", "ur", "vi", "zh",
    // Low tier (Whisper-supported but not yet surfaced in the UI). Listed
    // here so a power-user manually editing settings.json can still get a
    // valid Language; the frontend will simply have no flag/display name.
    "af", "am", "as", "ba", "bn", "bo", "br", "fo", "ha", "haw", "ht", "jw", "kn", "la", "lb", "ln",
    "mg", "mi", "nn", "oc", "pa", "ps", "sa", "sd", "si", "sn", "so", "su", "tg", "tk", "tl", "tt",
    "uz", "yi", "yo",
];

/// Persisted/serialised language code. Newtype around `String` so we keep
/// type-safety at call sites without locking ourselves into a closed enum
/// when we add languages. `#[serde(transparent)]` keeps the on-disk wire
/// format identical to the previous enum (`#[serde(rename_all = "lowercase")]`),
/// so existing settings.json files migrate without touching the user.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Language(pub String);

impl Language {
    /// The auto-detect sentinel.
    pub fn auto() -> Self {
        Self("auto".to_string())
    }

    /// ISO 639-1 code expected by whisper-rs. `None` means auto-detect.
    pub fn to_whisper_code(&self) -> Option<&str> {
        if self.0 == "auto" {
            None
        } else {
            Some(self.0.as_str())
        }
    }

    /// Stable serialization code (the same string we store on disk).
    pub fn to_code(&self) -> &str {
        &self.0
    }

    /// Construct a `Language` from a code, returning `None` if it's not in
    /// the known Whisper language list. Use this on every untrusted input
    /// (Tauri command args, persisted settings) so we never feed garbage
    /// to the Whisper engine.
    pub fn from_code(code: &str) -> Option<Language> {
        if Self::is_known(code) {
            Some(Self(code.to_string()))
        } else {
            None
        }
    }

    /// `true` if `code` is a recognised Whisper language code (or `auto`).
    pub fn is_known(code: &str) -> bool {
        KNOWN_WHISPER_LANGUAGES.contains(&code)
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::auto()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub model: String,
    pub shortcut: String,
    /// Optional shortcut to cycle through favorite languages. Empty = unbound.
    #[serde(default)]
    pub language_toggle_shortcut: String,
    /// Optional shortcut to cycle through models compatible with current language.
    #[serde(default)]
    pub model_toggle_shortcut: String,
    /// Languages the user wants to cycle through with the language shortcut.
    /// Defaults to empty here — the frontend pushes its own default set
    /// (the `high` tier from `src/utils/languages.ts`) at init time.
    #[serde(default)]
    pub favorite_languages: Vec<Language>,
    /// Per-model language whitelist. Key = model name (e.g. "large-v3-turbo").
    /// A model that's missing from this map is treated as supporting every
    /// favorite language (friendly default for newly downloaded models).
    #[serde(default)]
    pub model_languages: HashMap<String, Vec<Language>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::auto(),
            model: "large-v3-turbo".to_string(),
            shortcut: "CommandOrControl+Shift+Space".to_string(),
            language_toggle_shortcut: String::new(),
            model_toggle_shortcut: String::new(),
            favorite_languages: Vec::new(),
            model_languages: HashMap::new(),
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
        assert_eq!(Language::auto().to_whisper_code(), None);
    }

    #[test]
    fn languages_map_to_iso_codes() {
        assert_eq!(
            Language::from_code("en").unwrap().to_whisper_code(),
            Some("en")
        );
        assert_eq!(
            Language::from_code("fr").unwrap().to_whisper_code(),
            Some("fr")
        );
        assert_eq!(
            Language::from_code("ja").unwrap().to_whisper_code(),
            Some("ja")
        );
        assert_eq!(
            Language::from_code("lv").unwrap().to_whisper_code(),
            Some("lv")
        );
    }

    #[test]
    fn known_set_includes_auto_and_high_tier() {
        assert!(Language::is_known("auto"));
        for code in ["en", "fr", "es", "de", "ja", "zh", "ru", "ar"] {
            assert!(Language::is_known(code), "{code} should be known");
        }
    }

    #[test]
    fn known_set_includes_medium_tier_user_added() {
        // Latvian was the prototype case for adding non-mainstream languages.
        for code in ["lv", "lt", "et", "ro", "hu", "th", "vi"] {
            assert!(Language::is_known(code), "{code} should be known");
        }
    }

    #[test]
    fn from_code_round_trips() {
        for code in ["auto", "en", "fr", "lv"] {
            let lang = Language::from_code(code).unwrap();
            assert_eq!(lang.to_code(), code);
        }
    }

    #[test]
    fn from_code_rejects_unknown() {
        assert_eq!(Language::from_code("zz"), None);
        assert_eq!(Language::from_code(""), None);
        assert_eq!(Language::from_code("not-a-code"), None);
    }
}
