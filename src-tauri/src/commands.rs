use crate::audio::AudioChunk;
use crate::state::{AppState, AppStatus, Language, Permissions};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

/// Get the models directory path
/// In dev mode: uses src-tauri/models/ in the project directory
/// In release mode: uses the bundled resources directory
fn get_models_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        // Dev mode: use local models folder
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        tracing::debug!("Executable path: {}", exe_path.display());
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
        // Release mode: use bundled resources directory
        let resource_dir = _app.path().resource_dir().map_err(|e| e.to_string())?;
        tracing::info!("[RELEASE] Resource directory: {}", resource_dir.display());
        let models_dir = resource_dir.join("models");
        tracing::info!("[RELEASE] Models directory: {}", models_dir.display());

        // Debug: list contents of resource dir
        if let Ok(entries) = std::fs::read_dir(&resource_dir) {
            tracing::info!("[RELEASE] Contents of resource_dir:");
            for entry in entries.flatten() {
                tracing::info!("  - {}", entry.path().display());
            }
        }

        // Debug: list contents of models dir if it exists
        if models_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&models_dir) {
                tracing::info!("[RELEASE] Contents of models_dir:");
                for entry in entries.flatten() {
                    tracing::info!("  - {}", entry.path().display());
                }
            }
        } else {
            tracing::warn!(
                "[RELEASE] Models directory does not exist: {}",
                models_dir.display()
            );
        }

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

    // Simplified naming: ggml-{model}.bin (no quantization in filename)
    let filename = format!("ggml-{}.bin", model);
    let models_dir = get_models_dir(&app)?;

    let model_path = models_dir.join(&filename);
    tracing::info!("Looking for model at: {}", model_path.display());

    if !model_path.exists() {
        tracing::error!("Model file not found: {}", model_path.display());
        return Err(format!(
            "Model not found: {}. Expected at: {}",
            filename,
            model_path.display()
        ));
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

    app.emit("model:loaded", &model)
        .map_err(|e| e.to_string())?;

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

// Settings commands
#[tauri::command]
pub fn set_model(name: String, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("Setting model: {}", name);
    state.update_settings(|s| {
        s.model = name;
    });
    Ok(())
}

#[tauri::command]
pub fn set_language(lang: String, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("Setting language: {}", lang);
    let language = match lang.as_str() {
        "en" => Language::En,
        "fr" => Language::Fr,
        "es" => Language::Es,
        "de" => Language::De,
        "it" => Language::It,
        "pt" => Language::Pt,
        "nl" => Language::Nl,
        "ja" => Language::Ja,
        "zh" => Language::Zh,
        "ko" => Language::Ko,
        "ar" => Language::Ar,
        "hi" => Language::Hi,
        "pl" => Language::Pl,
        _ => Language::Auto,
    };
    state.update_settings(|s| {
        s.language = language;
    });
    Ok(())
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

/// Update the global shortcut
#[tauri::command]
pub fn set_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    tracing::info!("Setting new shortcut: {}", shortcut);

    // Parse the new shortcut
    let new_shortcut: Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut format: {}", e))?;

    // Get the global shortcut manager
    let shortcut_manager = app.global_shortcut();

    // Unregister all existing shortcuts first
    if let Err(e) = shortcut_manager.unregister_all() {
        tracing::warn!("Failed to unregister existing shortcuts: {}", e);
    }

    // Register the new shortcut with handler
    // Note: on_shortcut both registers the shortcut AND sets the handler
    shortcut_manager
        .on_shortcut(new_shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                tracing::info!("Global shortcut triggered");
                if let Err(e) = _app.emit("shortcut:triggered", ()) {
                    tracing::error!("Failed to emit shortcut event: {}", e);
                }
            }
        })
        .map_err(|e| format!("Failed to register shortcut '{}': {}. It may already be used by another application.", shortcut, e))?;

    // Update the shortcut in state
    state.update_settings(|s| {
        s.shortcut = shortcut.clone();
    });

    tracing::info!("Global shortcut updated to: {}", shortcut);
    Ok(())
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
