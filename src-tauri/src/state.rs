use crate::audio::{AudioCapture, VoiceActivityDetector};
use crate::whisper::{ModelCapabilities, WhisperWorker};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// Filename of the persisted store inside the app data directory.
/// Matches the JS-side constant in `src/composables/useStore.ts` —
/// kept in sync because both sides ultimately address the same file.
pub const SETTINGS_STORE_FILE: &str = "settings.json";
/// Key inside the store under which the full Settings JSON lives.
/// The pre-v0.1.8 frontend wrote here too, so v0.1.7 settings.json
/// files load transparently.
pub const SETTINGS_STORE_KEY: &str = "settings";

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

/// A user-imported Whisper model. Persisted in `settings.json` under
/// `userModels`. The `id` is a freshly-minted UUID v4 (not derived
/// from the path) so renaming or moving the file post-import doesn't
/// break references; it stays stable across the app lifetime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserModel {
    pub id: String,
    pub display_name: String,
    pub path: PathBuf,
    pub added_at: i64,
    pub capabilities: ModelCapabilities,
}

/// One entry of the transcription history. Field names are
/// camelCase'd via the parent `Settings` struct's `rename_all`, so
/// the JSON shape matches the TypeScript `HistoryEntry` interface
/// produced by the v0.1.7 frontend (no migration needed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub text: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// Wire format = camelCase to match the TypeScript `PersistedSettings`
// interface. v0.1.7's settings.json was JS-written (camelCase), so
// keeping this rename keeps the file readable across versions.
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: Language,
    pub model: String,
    pub shortcut: String,
    /// Whether transcripts copy to the clipboard automatically after
    /// the `transcript:final` event. Frontend mirror: `autoCopy`.
    #[serde(default = "default_auto_copy")]
    pub auto_copy: bool,
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
    /// Behaviour of the language-cycle shortcut. Two literal values:
    /// `"model-first"` (keep the active model fixed, cycle only through
    /// favourites it supports — the v0.1.6 behaviour, default) and
    /// `"language-first"` (cycle through every favourite, auto-switching
    /// to the most-capable compatible model when needed).
    ///
    /// Stored as a `String` rather than an enum so that adding a future
    /// mode doesn't break old settings.json files. Validation lives in
    /// the `set_language_cycle_mode` command.
    #[serde(default = "default_language_cycle_mode")]
    pub language_cycle_mode: String,
    /// User-imported custom Whisper models. Empty for users who only
    /// rely on the built-in `small` and `large-v3-turbo` models.
    /// Order is the import order = the order they appear in the cycle.
    #[serde(default)]
    pub user_models: Vec<UserModel>,
    /// Model ids (built-in or user-imported) the user has marked as
    /// disabled. Disabled models are skipped by the cycle shortcuts
    /// but stay selectable manually via Settings (which implicitly
    /// re-enables them on click).
    #[serde(default)]
    pub disabled_models: Vec<String>,
    /// Transcription history (capped at `MAX_HISTORY` entries by the
    /// `add_history_entry` command). Persisted alongside Settings so
    /// the user's recent transcripts survive restarts.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// Whether the user dismissed the Vulkan-not-available warning
    /// permanently. Frontend mirror: `vulkanWarningDismissed`.
    #[serde(default)]
    pub vulkan_warning_dismissed: bool,
    /// Whether the user dismissed the first-launch welcome window
    /// permanently. Frontend mirror: `welcomeDismissed`.
    #[serde(default)]
    pub welcome_dismissed: bool,
}

fn default_auto_copy() -> bool {
    // v0.1.7 frontend default; keeping the same on-disk default so
    // an upgrade isn't a behaviour regression.
    true
}

fn default_language_cycle_mode() -> String {
    "model-first".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::auto(),
            model: "large-v3-turbo".to_string(),
            shortcut: "CommandOrControl+Shift+Space".to_string(),
            auto_copy: default_auto_copy(),
            language_toggle_shortcut: String::new(),
            model_toggle_shortcut: String::new(),
            favorite_languages: Vec::new(),
            model_languages: HashMap::new(),
            language_cycle_mode: default_language_cycle_mode(),
            user_models: Vec::new(),
            disabled_models: Vec::new(),
            history: Vec::new(),
            vulkan_warning_dismissed: false,
            welcome_dismissed: false,
        }
    }
}

impl Settings {
    /// Persist the current Settings to `settings.json` via
    /// `tauri-plugin-store`. Called by every mutator command after
    /// the in-memory mutation, so the disk and the AppState never
    /// drift. Returns the Tauri-plugin-store error stringified for
    /// easy `?`-propagation in command results.
    pub fn persist(&self, app: &AppHandle) -> Result<(), String> {
        let store = app
            .store(SETTINGS_STORE_FILE)
            .map_err(|e| format!("open store {SETTINGS_STORE_FILE}: {e}"))?;
        let value = serde_json::to_value(self).map_err(|e| format!("serialise Settings: {e}"))?;
        store.set(SETTINGS_STORE_KEY, value);
        store.save().map_err(|e| format!("save store: {e}"))?;
        Ok(())
    }

    /// Load Settings from `settings.json`. Returns a fresh
    /// `Settings::default()` when the file or the key is missing —
    /// that's the first-launch case and any legitimate I/O error
    /// is logged but doesn't block app boot.
    pub fn load_from_disk(app: &AppHandle) -> Self {
        let store = match app.store(SETTINGS_STORE_FILE) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("settings store unavailable, using defaults: {e}");
                return Self::default();
            }
        };
        let raw = match store.get(SETTINGS_STORE_KEY) {
            Some(v) => v,
            None => return Self::default(),
        };
        match serde_json::from_value::<Self>(raw) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "settings.json parse error, using defaults (the bad file is preserved on disk): {e}"
                );
                Self::default()
            }
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
    /// Transient set of model ids that failed to load this session.
    /// Not persisted: a fresh app launch is a natural opportunity to
    /// re-attempt (the file may have been fixed, the GPU driver
    /// updated, etc.). Cycle shortcuts skip ids in this set.
    pub broken_models: HashSet<String>,
}

impl Default for AppStateInner {
    fn default() -> Self {
        Self {
            status: AppStatus::Idle,
            settings: Settings::default(),
            permissions: Permissions::default(),
            vu_level: 0.0,
            broken_models: HashSet::new(),
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

    // ---- Custom-model registry helpers --------------------------------
    // All three mutating helpers below take the write lock for the full
    // read-modify-write so two concurrent imports can't lose entries.

    /// Push a new user model. Returns `Err` if a model with the same
    /// canonical path is already imported (same file twice).
    pub fn add_user_model(&self, model: UserModel) -> Result<(), String> {
        let mut inner = self.inner.write();
        if let Some(existing) = inner
            .settings
            .user_models
            .iter()
            .find(|m| m.path == model.path)
        {
            return Err(existing.display_name.clone());
        }
        inner.settings.user_models.push(model);
        Ok(())
    }

    /// Remove a user model by id. Returns `true` if found and removed.
    /// Built-in models cannot be removed via this path.
    pub fn remove_user_model(&self, id: &str) -> bool {
        let mut inner = self.inner.write();
        let before = inner.settings.user_models.len();
        inner.settings.user_models.retain(|m| m.id != id);
        // Also drop it from the disabled list if present.
        inner.settings.disabled_models.retain(|d| d != id);
        // Clear any transient broken flag.
        inner.broken_models.remove(id);
        inner.settings.user_models.len() != before
    }

    /// Set or clear the disabled flag for a model id (built-in or
    /// custom). The set is stored as a list to keep the JSON
    /// representation stable.
    pub fn set_model_disabled(&self, id: &str, disabled: bool) {
        let mut inner = self.inner.write();
        let already = inner.settings.disabled_models.iter().any(|d| d == id);
        match (already, disabled) {
            (true, false) => inner.settings.disabled_models.retain(|d| d != id),
            (false, true) => inner.settings.disabled_models.push(id.to_string()),
            _ => {}
        }
    }

    pub fn is_model_disabled(&self, id: &str) -> bool {
        self.inner
            .read()
            .settings
            .disabled_models
            .iter()
            .any(|d| d == id)
    }

    /// Mark a model id as broken (failed to load this session). Cycle
    /// shortcuts will skip it until cleared. Not persisted.
    pub fn set_model_broken(&self, id: &str) {
        self.inner.write().broken_models.insert(id.to_string());
    }

    pub fn clear_model_broken(&self, id: &str) {
        self.inner.write().broken_models.remove(id);
    }

    pub fn is_model_broken(&self, id: &str) -> bool {
        self.inner.read().broken_models.contains(id)
    }

    /// Look up a user-imported model by id.
    pub fn find_user_model(&self, id: &str) -> Option<UserModel> {
        self.inner
            .read()
            .settings
            .user_models
            .iter()
            .find(|m| m.id == id)
            .cloned()
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

    #[test]
    fn settings_default_preserves_v0_1_6_cycle_behaviour() {
        // Existing v0.1.6 users upgrading must not see their language-cycle
        // behaviour change without explicit consent. Default = "model-first".
        assert_eq!(Settings::default().language_cycle_mode, "model-first");
    }

    #[test]
    fn settings_deserialise_real_v017_camelcase_payload() {
        // The pre-v0.1.8 frontend wrote settings.json via JS — keys
        // are camelCase, not snake_case. The Rust struct must parse
        // both the new and old fields, defaulting any v0.1.8 keys
        // (userModels / disabledModels / history) that didn't exist
        // before.
        let v017_json = r#"{
            "language": "fr",
            "model": "small",
            "autoCopy": true,
            "shortcut": "CommandOrControl+Shift+Space",
            "languageToggleShortcut": "",
            "modelToggleShortcut": "",
            "favoriteLanguages": ["auto", "en", "fr"],
            "modelLanguages": {},
            "languageCycleMode": "model-first",
            "vulkanWarningDismissed": false,
            "welcomeDismissed": true
        }"#;
        let parsed: Settings = serde_json::from_str(v017_json).expect("v0.1.7 settings must parse");
        assert_eq!(parsed.language.to_code(), "fr");
        assert_eq!(parsed.model, "small");
        assert!(parsed.auto_copy);
        assert_eq!(parsed.favorite_languages.len(), 3);
        assert!(parsed.welcome_dismissed);
        // v0.1.8 additions default cleanly.
        assert!(parsed.user_models.is_empty());
        assert!(parsed.disabled_models.is_empty());
        assert!(parsed.history.is_empty());
    }

    #[test]
    fn settings_round_trips_through_serde_json() {
        // Exercises the in-memory persist path used by `Settings::persist`:
        // a Settings → JSON → Settings round-trip must preserve every
        // field. (The actual `tauri-plugin-store` IO is tested
        // end-to-end via the dev smoke gate.)
        let mut s = Settings::default();
        s.language = Language::from_code("ja").unwrap();
        s.disabled_models.push("small".to_string());
        s.history.push(HistoryEntry {
            id: "abc".into(),
            text: "hello".into(),
            timestamp: 1_700_000_000,
            model_id: Some("small".into()),
            duration_ms: Some(1234),
        });
        let json = serde_json::to_string(&s).expect("serialise");
        let back: Settings = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.language.to_code(), "ja");
        assert_eq!(back.disabled_models, vec!["small".to_string()]);
        assert_eq!(back.history.len(), 1);
        assert_eq!(back.history[0].id, "abc");
        assert_eq!(back.history[0].duration_ms, Some(1234));
    }

    #[test]
    fn add_user_model_rejects_duplicate_path() {
        let state = AppState::new();
        let m1 = UserModel {
            id: "id-1".to_string(),
            display_name: "first".to_string(),
            path: PathBuf::from("/tmp/whisper.bin"),
            added_at: 0,
            capabilities: ModelCapabilities {
                is_multilingual: true,
                size_class: "small".to_string(),
                quant_label: "q5_1".to_string(),
                n_vocab: 51865,
                n_audio_state: 768,
                n_audio_layer: 12,
                file_size_bytes: 1000,
            },
        };
        state.add_user_model(m1.clone()).expect("first add ok");

        // Same path, different id+display name → must be rejected with
        // the existing display name returned.
        let m2 = UserModel {
            id: "id-2".to_string(),
            display_name: "second".to_string(),
            ..m1
        };
        let err = state.add_user_model(m2).expect_err("duplicate path");
        assert_eq!(err, "first");
    }

    #[test]
    fn remove_user_model_clears_disabled_and_broken() {
        let state = AppState::new();
        let id = "to-remove".to_string();
        state
            .add_user_model(UserModel {
                id: id.clone(),
                display_name: "x".to_string(),
                path: PathBuf::from("/tmp/x.bin"),
                added_at: 0,
                capabilities: ModelCapabilities {
                    is_multilingual: true,
                    size_class: "small".to_string(),
                    quant_label: "q5_1".to_string(),
                    n_vocab: 51865,
                    n_audio_state: 768,
                    n_audio_layer: 12,
                    file_size_bytes: 1000,
                },
            })
            .unwrap();
        state.set_model_disabled(&id, true);
        state.set_model_broken(&id);

        assert!(state.remove_user_model(&id));
        assert!(!state.is_model_disabled(&id));
        assert!(!state.is_model_broken(&id));
        assert!(state.find_user_model(&id).is_none());
    }

    #[test]
    fn set_model_disabled_is_idempotent() {
        let state = AppState::new();
        state.set_model_disabled("small", true);
        state.set_model_disabled("small", true); // no double-add
        assert!(state.is_model_disabled("small"));
        let count = state
            .get_settings()
            .disabled_models
            .iter()
            .filter(|d| *d == "small")
            .count();
        assert_eq!(count, 1);

        state.set_model_disabled("small", false);
        assert!(!state.is_model_disabled("small"));
    }

    #[test]
    fn broken_models_are_transient_and_per_id() {
        let state = AppState::new();
        state.set_model_broken("a");
        state.set_model_broken("b");
        assert!(state.is_model_broken("a"));
        assert!(state.is_model_broken("b"));
        state.clear_model_broken("a");
        assert!(!state.is_model_broken("a"));
        assert!(state.is_model_broken("b"));
    }
}
