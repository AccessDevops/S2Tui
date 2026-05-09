use crate::audio::AudioChunk;
use crate::state::{AppState, AppStatus, Language, Permissions, Settings};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// Static description of every Whisper model the app knows how to fetch.
/// Each entry maps a short model id (the same one we persist in settings
/// and pass to whisper-rs) to its canonical filename, the public URL of
/// the bundled GitHub Release `models-v1` and the SHA-256 of the file
/// hosted there. Keep this in sync with the GitHub Release contents — if
/// you re-cut `models-v1` you must update both the URL tag (if you
/// rename it) and the SHA below.
struct ModelEntry {
    /// The id we accept in commands and store in settings.
    id: &'static str,
    /// Canonical filename inside `<app_data_dir>/models/` (and inside the
    /// dev-mode `src-tauri/models/` folder).
    filename: &'static str,
    /// Stable public URL on the `models-v1` GitHub Release.
    url: &'static str,
    /// Hex-encoded SHA-256 of the file at `url`. Verified after every
    /// download; mismatch deletes the partial file and surfaces an error.
    sha256: &'static str,
    /// Human-friendly label used in download progress events / UI.
    display_name: &'static str,
    /// Total size in bytes (used for progress when the response doesn't
    /// expose a Content-Length, e.g. transparently-compressed responses).
    size_bytes: u64,
    /// Pre-known capabilities for the built-in. We can't run the
    /// validator on these on app boot because the file might not be
    /// downloaded yet; values are taken from the upstream Whisper
    /// architecture spec and the quantisation variant we ship.
    is_multilingual: bool,
    n_vocab: i32,
    n_audio_state: i32,
    n_audio_layer: i32,
    quant_label: &'static str,
    size_class: &'static str,
}

const MODEL_REGISTRY: &[ModelEntry] = &[
    ModelEntry {
        id: "small",
        filename: "ggml-small.bin",
        url: "https://github.com/AccessDevops/S2Tui/releases/download/models-v1/ggml-small.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        display_name: "Small",
        size_bytes: 190085487,
        is_multilingual: true,
        n_vocab: 51865,
        n_audio_state: 768,
        n_audio_layer: 12,
        quant_label: "q5_1",
        size_class: "small",
    },
    ModelEntry {
        id: "large-v3-turbo",
        filename: "ggml-large-v3-turbo.bin",
        url: "https://github.com/AccessDevops/S2Tui/releases/download/models-v1/ggml-large-v3-turbo.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        display_name: "Large V3 Turbo",
        size_bytes: 601463531,
        is_multilingual: true,
        n_vocab: 51866,
        // Turbo shares the encoder of large-v3 (n_audio_state=1280) but
        // ships a 4-layer decoder. We surface it as size_class="large"
        // for the user-facing label since perf characteristics line up
        // with large more than with medium/small.
        n_audio_state: 1280,
        n_audio_layer: 4,
        quant_label: "q5_0",
        size_class: "large",
    },
];

/// Resolve a model id to its on-disk path, dispatching between
/// built-in (MODEL_REGISTRY) and user-imported (Settings.user_models).
/// Used by every `load_*` / `download_*` command — without this,
/// custom-model ids would always map to a non-existent
/// `<models_dir>/ggml-<uuid>.bin` and load attempts would fail
/// instantly, surfacing as a "broken" pill in the UI even though the
/// file is fine.
fn resolve_model_path(
    state: &AppState,
    app: &AppHandle,
    model_id: &str,
) -> Result<PathBuf, String> {
    // Built-in lookup first — keeps the hot path identical to the
    // pre-custom-models behaviour for existing users.
    if let Some(entry) = MODEL_REGISTRY.iter().find(|e| e.id == model_id) {
        return Ok(get_models_dir(app)?.join(entry.filename));
    }
    // Custom user-imported model. The path is whatever the user
    // picked at import time, stored canonical inside Settings.
    if let Some(user_model) = state.find_user_model(model_id) {
        return Ok(user_model.path);
    }
    Err(format!("Unknown model id: {model_id}"))
}

/// Build the `ModelCapabilities` value for a built-in entry. Mirrors
/// the shape returned by `whisper::compat::validate` for user-imported
/// models so the frontend treats both kinds uniformly.
fn builtin_capabilities(entry: &ModelEntry) -> crate::whisper::ModelCapabilities {
    crate::whisper::ModelCapabilities {
        is_multilingual: entry.is_multilingual,
        size_class: entry.size_class.to_string(),
        quant_label: entry.quant_label.to_string(),
        n_vocab: entry.n_vocab,
        n_audio_state: entry.n_audio_state,
        n_audio_layer: entry.n_audio_layer,
        file_size_bytes: entry.size_bytes,
    }
}

/// Resolve the directory in which Whisper models live.
///
/// Dev mode (`#[cfg(debug_assertions)]`) keeps reading `src-tauri/models/`
/// directly so a maintainer who already has the bins on disk doesn't have
/// to re-download anything.
///
/// Release mode now points at `<app_data_dir>/models/` — a writable
/// per-user directory created on demand. Models are *no longer* shipped
/// inside the bundle's `Resources/`; the app downloads them on first
/// launch via `download_model` (see below), so the directory is the
/// single mutable cache.
fn get_models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let _ = app; // unused in dev mode
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        let project_root = exe_path
            .parent() // target/debug
            .and_then(|p| p.parent()) // target
            .and_then(|p| p.parent()) // src-tauri
            .ok_or("Could not find project root")?;
        let models_dir = project_root.join("models");
        tracing::info!("[DEV] Models directory: {}", models_dir.display());
        Ok(models_dir)
    }
    #[cfg(not(debug_assertions))]
    {
        let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let models_dir = app_data.join("models");
        if !models_dir.exists() {
            std::fs::create_dir_all(&models_dir).map_err(|e| {
                format!(
                    "Failed to create models dir {}: {}",
                    models_dir.display(),
                    e
                )
            })?;
        }
        tracing::info!("[RELEASE] Models directory: {}", models_dir.display());
        Ok(models_dir)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListenMode {
    Toggle,
    PushToTalk,
    VoiceActivated,
}

// Audio commands
#[tauri::command]
pub async fn start_listen(
    mode: ListenMode,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    tracing::info!("Starting listen with mode: {:?}", mode);

    // Check permissions first
    let perms = state.get_permissions();
    if !perms.microphone {
        app.emit("permission:required", "microphone")
            .map_err(|e| e.to_string())?;
        return Err("Microphone permission required".to_string());
    }

    // Start audio capture
    let audio_capture = Arc::clone(&state.audio_capture);
    let chunk_rx = audio_capture.create_chunk_channel();

    audio_capture.start().map_err(|e| {
        tracing::error!("Failed to start audio capture: {}", e);
        e.to_string()
    })?;

    state.set_status(AppStatus::Listening);
    app.emit("state:change", "listening")
        .map_err(|e| e.to_string())?;

    // Spawn VAD processing task
    let vad = Arc::clone(&state.vad);
    let app_clone = app.clone();
    tokio::spawn(process_audio_chunks(chunk_rx, vad, app_clone));

    Ok(())
}

#[tauri::command]
pub async fn stop_listen(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    tracing::info!("Stopping listen");

    state.set_status(AppStatus::Processing);
    app.emit("state:change", "processing")
        .map_err(|e| e.to_string())?;

    // Small delay to ensure the "processing" state is visible in the UI
    // This prevents Vue from batching the state changes
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Stop audio capture and get samples
    let samples = state.audio_capture.stop().map_err(|e| e.to_string())?;

    // Reset VAD state
    state.vad.write().reset();

    let samples_count = samples.len();
    let duration = samples_count as f32 / 16000.0;
    tracing::info!(
        "Captured {:.2}s of audio ({} samples)",
        duration,
        samples_count
    );

    // Check minimum duration
    if duration < 0.5 {
        state.set_status(AppStatus::Idle);
        app.emit("state:change", "idle")
            .map_err(|e| e.to_string())?;
        return Err("Recording too short".to_string());
    }

    // Transcribe with Whisper
    let whisper = state.whisper.clone();
    let transcribe_start = std::time::Instant::now();
    let transcription = tokio::task::spawn_blocking(move || whisper.transcribe(&samples))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())?;
    let transcribe_duration_ms = transcribe_start.elapsed().as_millis() as u64;

    // Get current model from settings
    let current_model = state.get_settings().model.clone();

    app.emit(
        "transcript:final",
        serde_json::json!({
            "text": transcription,
            "duration": duration,
            "samples": samples_count,
            "model": current_model,
            "transcribeDurationMs": transcribe_duration_ms
        }),
    )
    .map_err(|e| e.to_string())?;

    state.set_status(AppStatus::Idle);
    app.emit("state:change", "idle")
        .map_err(|e| e.to_string())?;

    Ok(transcription)
}

#[tauri::command]
pub async fn load_whisper_model(
    model: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    tracing::info!("Loading Whisper model: {}", model);

    // Resolve via the shared helper so user-imported (uuid-keyed)
    // ids land on their actual stored path, not a synthesised
    // `ggml-<uuid>.bin` that doesn't exist.
    let model_path = resolve_model_path(&state, &app, &model)?;
    tracing::info!("Looking for model at: {}", model_path.display());

    if !model_path.exists() {
        tracing::error!("Model file not found: {}", model_path.display());
        return Err(format!("Model file not found at {}", model_path.display()));
    }

    tracing::info!("Model file found, loading...");

    // Load model in a blocking task
    let whisper = state.whisper.clone();
    tokio::task::spawn_blocking(move || whisper.load_model(model_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())?;

    // Update settings
    state.update_settings(|s| {
        s.model = model.clone();
    });

    // Re-apply the current language selection to the whisper engine after a
    // model load, so the engine config stays in sync with user settings even
    // if set_language ran before the model was ready.
    let whisper_code = state
        .get_settings()
        .language
        .to_whisper_code()
        .map(String::from);
    state.whisper.set_language(whisper_code.clone());
    tracing::info!(
        "Whisper language re-applied after model load: {}",
        whisper_code.as_deref().unwrap_or("auto-detect")
    );

    app.emit("model:loaded", &model)
        .map_err(|e| e.to_string())?;

    persist_and_broadcast(&state, &app)?;

    tracing::info!("Whisper model loaded successfully: {}", model);
    Ok(())
}

#[tauri::command]
pub fn is_model_loaded(state: State<'_, AppState>) -> bool {
    state.whisper.is_loaded()
}

/// Process audio chunks and emit VAD levels
async fn process_audio_chunks(
    mut rx: mpsc::UnboundedReceiver<AudioChunk>,
    vad: Arc<RwLock<crate::audio::VoiceActivityDetector>>,
    app: AppHandle,
) {
    tracing::info!("VAD processing started");

    while let Some(chunk) = rx.recv().await {
        // Process with VAD
        let result = vad.write().process(&chunk.samples);

        // Emit VAD level to frontend
        let _ = app.emit(
            "vad:level",
            serde_json::json!({
                "rms": result.rms_level,
                "isSpeech": result.is_speech
            }),
        );
    }

    tracing::info!("VAD processing stopped");
}

// =============================================================================
// Persisted-state plumbing — single source of truth lives in AppState; every
// mutator routes through `persist_and_broadcast` so disk and memory move
// together and every other window picks up the change via one event.
// =============================================================================

/// Write the current Settings to `settings.json` and emit
/// `settings:changed`. Called by every mutator command after the
/// in-memory mutation. The two operations are paired here (rather
/// than at each call site) so a future setter can't forget the
/// broadcast and silently leak desync between windows.
fn persist_and_broadcast(state: &AppState, app: &AppHandle) -> Result<(), String> {
    state.get_settings().persist(app)?;
    if let Err(e) = app.emit("settings:changed", ()) {
        tracing::warn!("settings:changed broadcast failed: {e}");
    }
    Ok(())
}

/// Read the full Settings out of AppState. Frontend windows call
/// this on boot (via `useSettingsSync`) and on every
/// `settings:changed` event to refresh their local Pinia cache.
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.get_settings()
}

/// Cap on how many history entries we keep. Mirrors the JS-side
/// `MAX_HISTORY` so behaviour is identical to v0.1.7.
const MAX_HISTORY: usize = 20;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddHistoryEntry {
    pub text: String,
    pub model_id: Option<String>,
    pub duration_ms: Option<u64>,
}

/// Prepend a transcription to the history list, capped at
/// `MAX_HISTORY`. Returns the freshly-created entry so the caller
/// can use the id for further operations (e.g. delete) without
/// another round-trip.
#[tauri::command]
pub fn add_history_entry(
    entry: AddHistoryEntry,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<crate::state::HistoryEntry, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let new_entry = crate::state::HistoryEntry {
        id: timestamp.to_string(),
        text: entry.text,
        timestamp,
        model_id: entry.model_id,
        duration_ms: entry.duration_ms,
    };
    state.update_settings(|s| {
        s.history.insert(0, new_entry.clone());
        if s.history.len() > MAX_HISTORY {
            s.history.truncate(MAX_HISTORY);
        }
    });
    persist_and_broadcast(&state, &app)?;
    Ok(new_entry)
}

/// Drop every entry from the history.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    state.update_settings(|s| s.history.clear());
    persist_and_broadcast(&state, &app)
}

// Settings commands
#[tauri::command]
pub fn set_model(name: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    tracing::info!("Setting model: {}", name);
    state.update_settings(|s| {
        s.model = name;
    });
    persist_and_broadcast(&state, &app)
}

#[tauri::command]
pub fn set_language(
    lang: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    tracing::info!("Setting language: {}", lang);
    // Validate against the canonical Whisper language list. Unknown codes
    // collapse to auto-detect rather than crashing or silently selecting the
    // wrong language.
    let language = Language::from_code(&lang).unwrap_or_else(Language::auto);
    let whisper_code = language.to_whisper_code().map(String::from);
    state.update_settings(|s| {
        s.language = language;
    });

    // Propagate the selection to the whisper engine. Without this, the engine
    // keeps its default (`None` = auto-detect) regardless of the UI choice.
    state.whisper.set_language(whisper_code.clone());
    tracing::info!(
        "Whisper language set to: {}",
        whisper_code.as_deref().unwrap_or("auto-detect")
    );

    persist_and_broadcast(&state, &app)
}

// Permission commands
#[tauri::command]
pub fn check_permissions(state: State<'_, AppState>) -> Permissions {
    let microphone = check_microphone_permission();

    let perms = Permissions { microphone };
    state.set_permissions(perms.clone());
    perms
}

/// Check if microphone permission is granted
fn check_microphone_permission() -> bool {
    crate::platform::is_microphone_authorized()
}

/// Request microphone permission from the system
/// On macOS, this triggers the native permission dialog
/// Returns true if permission was granted
#[tauri::command]
pub async fn request_microphone_permission(state: State<'_, AppState>) -> Result<bool, String> {
    tracing::info!("Requesting microphone permission");

    // Run in blocking task since it waits for user response
    let granted = tokio::task::spawn_blocking(crate::platform::request_microphone_permission)
        .await
        .map_err(|e| format!("Task join error: {}", e))?;

    // Update permissions state
    if granted {
        state.set_permissions(Permissions { microphone: true });
    }

    tracing::info!("Microphone permission granted: {}", granted);
    Ok(granted)
}

/// Re-register every shortcut declared in settings (main + language toggle + model toggle).
/// Always replaces the whole set atomically: any change to one shortcut routes through here so
/// that we never end up with a stale registration referencing the wrong key combination.
pub fn register_all_shortcuts(app: &AppHandle, state: &AppState) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    let settings = state.get_settings();
    let shortcut_manager = app.global_shortcut();

    if let Err(e) = shortcut_manager.unregister_all() {
        tracing::warn!("Failed to unregister existing shortcuts: {}", e);
    }

    if !settings.shortcut.is_empty() {
        let main_shortcut: Shortcut = settings
            .shortcut
            .parse()
            .map_err(|e| format!("Invalid main shortcut format: {}", e))?;

        shortcut_manager
            .on_shortcut(main_shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    tracing::info!("Main shortcut triggered");
                    if let Err(e) = app.emit("shortcut:triggered", ()) {
                        tracing::error!("Failed to emit shortcut event: {}", e);
                    }
                }
            })
            .map_err(|e| {
                format!(
                    "Failed to register main shortcut '{}': {}. It may already be used by another application.",
                    settings.shortcut, e
                )
            })?;
        tracing::info!("Main shortcut registered: {}", settings.shortcut);
    }

    if !settings.language_toggle_shortcut.is_empty() {
        let lang_shortcut: Shortcut = settings
            .language_toggle_shortcut
            .parse()
            .map_err(|e| format!("Invalid language toggle shortcut format: {}", e))?;

        shortcut_manager
            .on_shortcut(lang_shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    tracing::info!("Language toggle shortcut triggered");
                    if let Err(e) = app.emit("shortcut:toggle-language", ()) {
                        tracing::error!("Failed to emit language toggle event: {}", e);
                    }
                }
            })
            .map_err(|e| {
                format!(
                    "Failed to register language toggle shortcut '{}': {}",
                    settings.language_toggle_shortcut, e
                )
            })?;
        tracing::info!(
            "Language toggle shortcut registered: {}",
            settings.language_toggle_shortcut
        );
    }

    if !settings.model_toggle_shortcut.is_empty() {
        let model_shortcut: Shortcut = settings
            .model_toggle_shortcut
            .parse()
            .map_err(|e| format!("Invalid model toggle shortcut format: {}", e))?;

        shortcut_manager
            .on_shortcut(model_shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    tracing::info!("Model toggle shortcut triggered");
                    if let Err(e) = app.emit("shortcut:toggle-model", ()) {
                        tracing::error!("Failed to emit model toggle event: {}", e);
                    }
                }
            })
            .map_err(|e| {
                format!(
                    "Failed to register model toggle shortcut '{}': {}",
                    settings.model_toggle_shortcut, e
                )
            })?;
        tracing::info!(
            "Model toggle shortcut registered: {}",
            settings.model_toggle_shortcut
        );
    }

    Ok(())
}

/// Update the main listen shortcut. Persists the new value and re-registers every shortcut.
#[tauri::command]
pub fn set_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Setting main shortcut: {}", shortcut);
    state.update_settings(|s| {
        s.shortcut = shortcut.clone();
    });
    register_all_shortcuts(&app, &state)?;
    persist_and_broadcast(&state, &app)
}

/// Update the language-cycle shortcut (empty string clears it).
#[tauri::command]
pub fn set_language_toggle_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Setting language toggle shortcut: {}", shortcut);
    state.update_settings(|s| {
        s.language_toggle_shortcut = shortcut.clone();
    });
    register_all_shortcuts(&app, &state)?;
    persist_and_broadcast(&state, &app)
}

/// Update the model-cycle shortcut (empty string clears it).
#[tauri::command]
pub fn set_model_toggle_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Setting model toggle shortcut: {}", shortcut);
    state.update_settings(|s| {
        s.model_toggle_shortcut = shortcut.clone();
    });
    register_all_shortcuts(&app, &state)?;
    persist_and_broadcast(&state, &app)
}

/// Update the favorite languages cycled by the language shortcut.
/// Codes that don't match a known [`Language`] are silently dropped.
#[tauri::command]
pub fn set_favorite_languages(
    languages: Vec<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let parsed: Vec<Language> = languages
        .iter()
        .filter_map(|code| Language::from_code(code))
        .collect();
    tracing::info!(
        "Setting favorite languages: {:?}",
        parsed.iter().map(|l| l.to_code()).collect::<Vec<_>>()
    );
    state.update_settings(|s| {
        s.favorite_languages = parsed;
    });
    persist_and_broadcast(&state, &app)
}

/// Replace the whitelist of languages enabled for a single model.
/// Useful for fine-tuned per-language models (e.g. a Latvian-only model).
#[tauri::command]
pub fn set_model_languages(
    model: String,
    languages: Vec<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let parsed: Vec<Language> = languages
        .iter()
        .filter_map(|code| Language::from_code(code))
        .collect();
    tracing::info!(
        "Setting languages for model '{}': {:?}",
        model,
        parsed.iter().map(|l| l.to_code()).collect::<Vec<_>>()
    );
    state.update_settings(|s| {
        s.model_languages.insert(model, parsed);
    });
    persist_and_broadcast(&state, &app)
}

/// Update the language-cycle behaviour. Two literal values are accepted:
/// `"model-first"` (the v0.1.6 default — model is sticky, cycle filtered
/// to favourites the current model supports) and `"language-first"`
/// (cycle through every favourite, auto-switching to the most-capable
/// compatible model when needed). Any other input is rejected so a
/// hand-edited settings.json with garbage doesn't reach the listener.
#[tauri::command]
pub fn set_language_cycle_mode(
    mode: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    if mode != "model-first" && mode != "language-first" {
        return Err(format!(
            "Invalid language cycle mode: {} (expected 'model-first' or 'language-first')",
            mode
        ));
    }
    tracing::info!("Language cycle mode set to: {}", mode);
    state.update_settings(|s| {
        s.language_cycle_mode = mode;
    });
    persist_and_broadcast(&state, &app)
}

/// Toggle the auto-copy-to-clipboard behaviour. v0.1.7 had this
/// living in JS; centralising it here keeps the Settings struct as
/// the sole source of truth.
#[tauri::command]
pub fn set_auto_copy(
    enabled: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    state.update_settings(|s| s.auto_copy = enabled);
    persist_and_broadcast(&state, &app)
}

/// Persist that the user dismissed the Vulkan-not-available warning.
/// v0.1.7 wrote this directly via the JS plugin-store; same idea as
/// `set_auto_copy`.
#[tauri::command]
pub fn set_vulkan_warning_dismissed(
    dismissed: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    state.update_settings(|s| s.vulkan_warning_dismissed = dismissed);
    persist_and_broadcast(&state, &app)
}

/// Persist that the user dismissed the welcome window.
#[tauri::command]
pub fn set_welcome_dismissed(
    dismissed: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    state.update_settings(|s| s.welcome_dismissed = dismissed);
    persist_and_broadcast(&state, &app)
}

/// Get list of available models on disk
/// Dynamically scans for ggml-*.bin files and extracts model names
#[tauri::command]
pub fn get_available_models(app: AppHandle) -> Result<Vec<String>, String> {
    let models_dir = get_models_dir(&app)?;
    tracing::info!("Scanning for models in: {}", models_dir.display());

    let mut available = Vec::new();

    // Check if models directory exists
    if !models_dir.exists() {
        tracing::warn!("Models directory does not exist: {}", models_dir.display());
        return Ok(available);
    }

    // Scan directory for ggml-*.bin files
    match std::fs::read_dir(&models_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Match pattern: ggml-{model_id}.bin
                    if filename.starts_with("ggml-") && filename.ends_with(".bin") {
                        // Extract model_id: remove "ggml-" prefix and ".bin" suffix
                        let model_id = &filename[5..filename.len() - 4];
                        tracing::info!("Found model: {} (file: {})", model_id, filename);
                        available.push(model_id.to_string());
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to read models directory: {}", e);
            return Err(format!("Failed to read models directory: {}", e));
        }
    }

    // Sort for consistent ordering
    available.sort();

    tracing::info!("Available models: {:?}", available);
    Ok(available)
}

/// Get GPU backend information
#[tauri::command]
pub fn get_gpu_info() -> crate::whisper::GpuInfo {
    crate::whisper::GpuInfo::detect()
}

/// Check system health (GPU/Vulkan availability)
#[tauri::command]
pub fn check_system_health() -> crate::whisper::SystemHealthCheck {
    crate::whisper::check_system_health()
}

/// GPU status information for the frontend
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    /// Is GPU being used for transcription?
    pub using_gpu: bool,
    /// Current backend name
    pub backend: String,
    /// Was fallback to CPU used?
    pub fallback_used: bool,
}

/// Get current GPU status
#[tauri::command]
pub fn get_gpu_status(state: State<'_, AppState>) -> GpuStatus {
    GpuStatus {
        using_gpu: state.whisper.is_using_gpu(),
        backend: state.whisper.get_backend_name(),
        fallback_used: state.whisper.was_fallback_used(),
    }
}

/// Load Whisper model with explicit GPU/CPU control
#[tauri::command]
pub async fn load_whisper_model_with_options(
    model: String,
    force_cpu: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<crate::whisper::ModelLoadResult, String> {
    tracing::info!("Loading Whisper model: {} (force_cpu={})", model, force_cpu);

    // Same resolution as `load_whisper_model`: built-in or
    // user-imported, always via the shared helper.
    let model_path = resolve_model_path(&state, &app, &model)?;
    tracing::info!("Looking for model at: {}", model_path.display());

    if !model_path.exists() {
        tracing::error!("Model file not found: {}", model_path.display());
        return Err(format!("Model file not found at {}", model_path.display()));
    }

    tracing::info!("Model file found, loading with options...");

    // Load model with options in a blocking task
    let whisper = state.whisper.clone();
    let result =
        tokio::task::spawn_blocking(move || whisper.load_model_with_options(model_path, force_cpu))
            .await
            .map_err(|e| format!("Task join error: {}", e))?
            .map_err(|e| e.to_string())?;

    // Update settings
    state.update_settings(|s| {
        s.model = model.clone();
    });

    // Re-apply the current language selection to the whisper engine after a
    // model load, so the engine config stays in sync with user settings even
    // if set_language ran before the model was ready.
    let whisper_code = state
        .get_settings()
        .language
        .to_whisper_code()
        .map(String::from);
    state.whisper.set_language(whisper_code.clone());
    tracing::info!(
        "Whisper language re-applied after model load: {}",
        whisper_code.as_deref().unwrap_or("auto-detect")
    );

    // Emit events
    app.emit("model:loaded", &model)
        .map_err(|e| e.to_string())?;

    // Emit GPU status event
    if result.fallback_used {
        app.emit("health:gpu-fallback", &result)
            .map_err(|e| e.to_string())?;
    }

    tracing::info!(
        "Whisper model loaded: {} (using_gpu={}, backend={}, fallback={})",
        model,
        result.using_gpu,
        result.backend,
        result.fallback_used
    );

    persist_and_broadcast(&state, &app)?;

    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredModelInfo {
    pub id: String,
    pub display_name: String,
    pub filename: String,
    pub size_bytes: u64,
    pub url: String,
    pub present: bool,
}

/// List the models the app expects to find on disk, with a `present`
/// flag the frontend can use to decide whether to show the first-launch
/// download dialog. The order matches MODEL_REGISTRY (small first, then
/// large) so the dialog walks them sequentially.
#[tauri::command]
pub fn list_required_models(app: AppHandle) -> Result<Vec<RequiredModelInfo>, String> {
    let models_dir = get_models_dir(&app)?;
    let mut out = Vec::with_capacity(MODEL_REGISTRY.len());
    for entry in MODEL_REGISTRY {
        let path = models_dir.join(entry.filename);
        out.push(RequiredModelInfo {
            id: entry.id.to_string(),
            display_name: entry.display_name.to_string(),
            filename: entry.filename.to_string(),
            size_bytes: entry.size_bytes,
            url: entry.url.to_string(),
            present: path.is_file(),
        });
    }
    Ok(out)
}

/// Download a Whisper model into the app's models directory, streaming
/// the response so we can emit progress events to the frontend in
/// near-realtime. The file lands in a `.partial` sibling first, gets
/// SHA-256 verified, then is atomically renamed to its final name.
///
/// Events emitted (all carry the model id so the UI can route correctly
/// when several downloads run sequentially):
/// - `model:download:progress`  { model, bytesReceived, totalBytes, percent }
/// - `model:download:complete`  { model, path }
/// - `model:download:error`     { model, message }
#[tauri::command]
pub async fn download_model(model: String, app: AppHandle) -> Result<(), String> {
    let entry = MODEL_REGISTRY
        .iter()
        .find(|e| e.id == model)
        .ok_or_else(|| format!("Unknown model id: {}", model))?;

    let models_dir = get_models_dir(&app)?;
    let final_path = models_dir.join(entry.filename);
    let partial_path = models_dir.join(format!("{}.partial", entry.filename));

    tracing::info!(
        "Downloading model '{}' from {} -> {}",
        entry.id,
        entry.url,
        final_path.display()
    );

    // Helper: emit a typed error event and propagate as Result::Err.
    let emit_error = |app: &AppHandle, msg: &str| -> String {
        let _ = app.emit(
            "model:download:error",
            serde_json::json!({ "model": entry.id, "message": msg }),
        );
        msg.to_string()
    };

    // Inline async block lets us use `?` and still funnel every error
    // through the same `model:download:error` emitter.
    let do_download = async {
        // Sanity: clear any leftover partial from a previous interrupted run.
        if partial_path.exists() {
            tokio::fs::remove_file(&partial_path)
                .await
                .map_err(|e| format!("Failed to clear stale .partial: {}", e))?;
        }

        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| format!("HTTP client init failed: {}", e))?;
        let mut response = client
            .get(entry.url)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {} from server", response.status()));
        }

        // Prefer Content-Length if the redirected CDN exposes it; fall
        // back to the registry's size_bytes so the progress bar still
        // moves predictably even when the server doesn't tell us.
        let total_bytes = response.content_length().unwrap_or(entry.size_bytes);

        let mut file = tokio::fs::File::create(&partial_path)
            .await
            .map_err(|e| format!("Failed to open temp file: {}", e))?;
        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;
        let mut last_pct: u8 = u8::MAX;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("Network read error: {}", e))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Disk write error: {}", e))?;
            hasher.update(&chunk);
            downloaded = downloaded.saturating_add(chunk.len() as u64);

            // `checked_div` returns None when total is zero — clippy's
            // `manual_checked_ops` (Rust 1.95) flags the equivalent
            // `if x > 0 { a / x } else { 0 }` pattern.
            let pct = (downloaded.min(total_bytes) * 100)
                .checked_div(total_bytes)
                .unwrap_or(0) as u8;
            // Throttle: only emit when integer percent advances, plus one
            // last update at end. Avoids flooding the bridge with hundreds
            // of events for a 547 MB file.
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "model:download:progress",
                    serde_json::json!({
                        "model": entry.id,
                        "bytesReceived": downloaded,
                        "totalBytes": total_bytes,
                        "percent": pct,
                    }),
                );
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush temp file: {}", e))?;
        drop(file);

        // SHA-256 verification — protects against partial transfers and
        // bit-flips. Mismatch is a hard error: we'd rather fail loud than
        // hand a corrupt model to whisper.cpp.
        let actual = format!("{:x}", hasher.finalize());
        if actual != entry.sha256 {
            tokio::fs::remove_file(&partial_path).await.ok();
            return Err(format!(
                "Checksum mismatch (expected {}, got {})",
                entry.sha256, actual
            ));
        }

        tokio::fs::rename(&partial_path, &final_path)
            .await
            .map_err(|e| format!("Failed to finalize download: {}", e))?;
        Ok(final_path.clone())
    };

    match do_download.await {
        Ok(path) => {
            tracing::info!("Model '{}' downloaded to {}", entry.id, path.display());
            let _ = app.emit(
                "model:download:complete",
                serde_json::json!({ "model": entry.id, "path": path.display().to_string() }),
            );
            Ok(())
        }
        Err(msg) => {
            tracing::error!("Model '{}' download failed: {}", entry.id, msg);
            // Belt-and-braces: clean any leftover partial.
            let _ = tokio::fs::remove_file(&partial_path).await;
            Err(emit_error(&app, &msg))
        }
    }
}

// =============================================================================
// Custom model import — Step 3 commands
// =============================================================================

/// Unified view of every model the app exposes (built-in + user-imported)
/// returned by `list_all_models`. The frontend mirrors this into its
/// Pinia `models` slice so both kinds render through the same row UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfoResponse {
    pub id: String,
    pub display_name: String,
    /// `"builtin"` for the registry-defined models (small, large-v3-turbo),
    /// `"custom"` for user-imported entries.
    pub kind: String,
    pub capabilities: crate::whisper::ModelCapabilities,
    /// Whether the user has marked this model as disabled (excluded
    /// from the cycle shortcut).
    pub disabled: bool,
    /// Transient flag set when a load attempt failed this session.
    pub broken: bool,
    /// Custom models include the canonical path; built-ins don't
    /// (their path is derived from `get_models_dir`).
    pub path: Option<String>,
    /// Built-ins include their stable URL on the `models-v1` GitHub
    /// Release (used by the welcome window's first-launch download).
    pub url: Option<String>,
    /// Filename inside `<models_dir>/`. `None` for custom models since
    /// they're addressed by their absolute `path` instead.
    pub filename: Option<String>,
    /// Whether the file is on disk right now. Builds the
    /// `downloaded` flag in the frontend store. For custom models we
    /// check the imported path; for built-ins we check
    /// `<models_dir>/<filename>`.
    pub present: bool,
}

/// Return the merged list of built-in + user-imported models with
/// their capabilities and current disabled/broken state. Called once
/// on app boot by the frontend, then re-fetched whenever
/// `settings:updated` fires (so two windows stay in sync).
#[tauri::command]
pub fn list_all_models(
    state: State<AppState>,
    app: AppHandle,
) -> Result<Vec<ModelInfoResponse>, String> {
    let models_dir = get_models_dir(&app)?;
    let settings = state.get_settings();
    let mut out = Vec::with_capacity(MODEL_REGISTRY.len() + settings.user_models.len());

    for entry in MODEL_REGISTRY {
        let path = models_dir.join(entry.filename);
        out.push(ModelInfoResponse {
            id: entry.id.to_string(),
            display_name: entry.display_name.to_string(),
            kind: "builtin".to_string(),
            capabilities: builtin_capabilities(entry),
            disabled: state.is_model_disabled(entry.id),
            broken: state.is_model_broken(entry.id),
            path: None,
            url: Some(entry.url.to_string()),
            filename: Some(entry.filename.to_string()),
            present: path.is_file(),
        });
    }

    for um in &settings.user_models {
        out.push(ModelInfoResponse {
            id: um.id.clone(),
            display_name: um.display_name.clone(),
            kind: "custom".to_string(),
            capabilities: um.capabilities.clone(),
            disabled: state.is_model_disabled(&um.id),
            broken: state.is_model_broken(&um.id),
            path: Some(um.path.to_string_lossy().to_string()),
            url: None,
            filename: None,
            present: um.path.is_file(),
        });
    }

    Ok(out)
}

/// Run the file-format validator on a candidate path, then layer the
/// contextual checks (already-imported, inside-managed-dir) on top.
/// Returns the validation result on success or a structured error
/// the frontend can switch on by `kind` to render the right modal
/// state.
#[tauri::command]
pub fn validate_custom_model(
    state: State<AppState>,
    app: AppHandle,
    path: String,
) -> Result<crate::whisper::ValidationResult, crate::whisper::ModelCompatError> {
    let candidate = PathBuf::from(&path);

    // Refuse files inside our managed `models/` directory. They're
    // either already part of MODEL_REGISTRY (about to be downloaded)
    // or some leftover the user shouldn't be importing as a custom
    // entry. `get_models_dir` returns Err in obscure platform setups
    // — fall through silently in that case rather than blocking the
    // import.
    if let Ok(managed_dir) = get_models_dir(&app) {
        if let (Ok(canon_candidate), Ok(canon_managed)) =
            (candidate.canonicalize(), managed_dir.canonicalize())
        {
            if canon_candidate.starts_with(&canon_managed) {
                return Err(crate::whisper::ModelCompatError::InsideManagedDir);
            }
        }
    }

    // Reject duplicates by canonical path. `path.canonicalize()` will
    // fail for non-existent files, but the validator checks
    // existence anyway via `metadata`.
    let canon_candidate = candidate.canonicalize().ok();
    if let Some(canon) = canon_candidate.as_ref() {
        let settings = state.get_settings();
        if let Some(existing) = settings
            .user_models
            .iter()
            .find(|m| m.path.canonicalize().ok().as_ref() == Some(canon))
        {
            return Err(crate::whisper::ModelCompatError::AlreadyImported {
                existing_display_name: existing.display_name.clone(),
            });
        }
    }

    crate::whisper::compat::validate(&candidate)
}

/// Step 4 errors specific to the add/remove flows. File-format errors
/// from the validator path use `ModelCompatError`; these wrap them
/// plus the contextual cases (active-model deletion blocking,
/// mid-recording, etc.).
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AddModelError {
    /// The candidate file no longer validates — race window between
    /// the `validate_custom_model` call (when the modal opened) and
    /// the `add_custom_model` call (when the user hit Save).
    Compat(crate::whisper::ModelCompatError),
    /// Display name is empty / whitespace-only after trim.
    EmptyName,
    /// settings.json couldn't be written (disk full, permission issue,
    /// etc.). Surfaces the underlying message so the modal can show
    /// it; the in-memory AppState mutation is rolled back by virtue
    /// of the AppState write lock having already been released.
    PersistFailed { reason: String },
}

impl From<crate::whisper::ModelCompatError> for AddModelError {
    fn from(e: crate::whisper::ModelCompatError) -> Self {
        AddModelError::Compat(e)
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoveModelError {
    /// The user is mid-recording; switching model now would kill the
    /// in-flight transcription.
    MidRecording,
    /// The deleted model is currently active and we can't find an
    /// alternative (every other model disabled or broken).
    NoFallback,
    /// The id doesn't match a user-imported model. Built-ins can't
    /// be removed via this path.
    NotFound,
}

/// Persist a user-imported Whisper model. Re-runs the validator (the
/// file may have been replaced between the modal opening and the
/// Save click), then commits the new entry to AppState. Returns the
/// fully-populated `UserModel` so the frontend can persist
/// settings.json without an extra round-trip.
#[tauri::command]
pub fn add_custom_model(
    state: State<AppState>,
    app: AppHandle,
    name: String,
    path: String,
) -> Result<crate::state::UserModel, AddModelError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AddModelError::EmptyName);
    }

    let candidate = PathBuf::from(&path);
    let validation = crate::whisper::compat::validate(&candidate)?;

    // Canonicalise so duplicate detection survives `./relative` paths
    // and symlinks. Falls back to the original path on platforms
    // where canonicalize fails (rare on the systems we ship to but
    // worth handling).
    let canonical = candidate.canonicalize().unwrap_or(candidate);

    let added_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let model = crate::state::UserModel {
        id: uuid::Uuid::new_v4().to_string(),
        display_name: trimmed.to_string(),
        path: canonical,
        added_at,
        capabilities: validation.capabilities,
    };

    state
        .add_user_model(model.clone())
        .map_err(|existing_name| {
            AddModelError::Compat(crate::whisper::ModelCompatError::AlreadyImported {
                existing_display_name: existing_name,
            })
        })?;

    // English-only constraint: pre-populate the per-model language
    // whitelist with `["auto", "en"]` so the cycle shortcut and the
    // chip picker can treat non-English languages as "not allowed
    // for this model" via the existing `model_languages` plumbing.
    // The user can edit the whitelist later via the General tab.
    if !model.capabilities.is_multilingual {
        state.update_settings(|s| {
            s.model_languages.insert(
                model.id.clone(),
                vec![
                    crate::state::Language::auto(),
                    crate::state::Language::from_code("en")
                        .unwrap_or_else(crate::state::Language::auto),
                ],
            );
        });
    }

    persist_and_broadcast(&state, &app).map_err(|e| AddModelError::PersistFailed { reason: e })?;

    Ok(model)
}

/// Remove a user-imported model from the registry. Does NOT delete
/// the file on disk. If the removed model is currently active,
/// auto-switch to the first available alternative (built-in or
/// custom, downloaded + not disabled + not broken). Returns the new
/// active model id, or `None` if the removed model wasn't active.
#[tauri::command]
pub fn remove_custom_model(
    state: State<AppState>,
    app: AppHandle,
    id: String,
) -> Result<Option<String>, RemoveModelError> {
    // Refuse to mutate while a recording is in flight.
    if state.get_status() != AppStatus::Idle {
        return Err(RemoveModelError::MidRecording);
    }

    // Confirm the id matches a user model (built-ins aren't removable
    // through this path).
    if state.find_user_model(&id).is_none() {
        return Err(RemoveModelError::NotFound);
    }

    let active_was_target = state.get_settings().model == id;
    let new_active: Option<String> = if active_was_target {
        // Find the first available alternative. Order matters — we
        // walk the same order `list_all_models` exposes (built-ins
        // first, then user models). Built-ins must be on disk;
        // disabled/broken excluded; the model being deleted excluded.
        let models_dir = get_models_dir(&app).map_err(|_| RemoveModelError::NoFallback)?;

        let settings = state.get_settings();
        let mut candidate: Option<String> = None;

        for entry in MODEL_REGISTRY {
            if entry.id == id {
                continue;
            }
            let on_disk = models_dir.join(entry.filename).is_file();
            if !on_disk {
                continue;
            }
            if state.is_model_disabled(entry.id) || state.is_model_broken(entry.id) {
                continue;
            }
            candidate = Some(entry.id.to_string());
            break;
        }
        if candidate.is_none() {
            for um in &settings.user_models {
                if um.id == id {
                    continue;
                }
                if !um.path.is_file() {
                    continue;
                }
                if state.is_model_disabled(&um.id) || state.is_model_broken(&um.id) {
                    continue;
                }
                candidate = Some(um.id.clone());
                break;
            }
        }

        let next = candidate.ok_or(RemoveModelError::NoFallback)?;
        state.update_settings(|s| s.model = next.clone());
        Some(next)
    } else {
        None
    };

    // remove_user_model also clears the disabled/broken flags for
    // this id; cf. state.rs.
    state.remove_user_model(&id);

    persist_and_broadcast(&state, &app).map_err(|_| RemoveModelError::NoFallback)?;

    Ok(new_active)
}

/// Toggle the disabled flag on a model id. Applies to built-ins and
/// custom models alike. Disabled models are skipped by the cycle
/// shortcuts; selecting one via Settings re-enables it implicitly
/// (the frontend handles the implicit re-enable; this command is
/// the explicit toggle).
#[tauri::command]
pub fn set_model_disabled(
    state: State<AppState>,
    app: AppHandle,
    id: String,
    disabled: bool,
) -> Result<(), String> {
    state.set_model_disabled(&id, disabled);
    persist_and_broadcast(&state, &app)
}
