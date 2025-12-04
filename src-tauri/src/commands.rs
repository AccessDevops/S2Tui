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
        let project_root = exe_path
            .parent() // target/debug
            .and_then(|p| p.parent()) // target
            .and_then(|p| p.parent()) // src-tauri
            .ok_or("Could not find project root")?;
        Ok(project_root.join("models"))
    }
    #[cfg(not(debug_assertions))]
    {
        // Release mode: use bundled resources directory
        _app.path()
            .resource_dir()
            .map(|p| p.join("models"))
            .map_err(|e| e.to_string())
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
    quant: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    tracing::info!("Loading Whisper model: {} ({})", model, quant);

    let filename = format!("ggml-{}-{}.bin", model, quant);
    let models_dir = get_models_dir(&app)?;
    tracing::info!("Models directory: {}", models_dir.display());

    let model_path = models_dir.join(&filename);

    if !model_path.exists() {
        return Err(format!(
            "Model not found: {}. Please download it first.",
            filename
        ));
    }

    // Load model in a blocking task
    let whisper = state.whisper.clone();
    tokio::task::spawn_blocking(move || whisper.load_model(model_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())?;

    // Update settings
    state.update_settings(|s| {
        s.model = model.clone();
        s.quantization = quant.clone();
    });

    app.emit("model:loaded", &model)
        .map_err(|e| e.to_string())?;

    tracing::info!("Whisper model loaded successfully");
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
pub fn set_model(name: String, quant: String, state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("Setting model: {} ({})", name, quant);
    state.update_settings(|s| {
        s.model = name;
        s.quantization = quant;
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
    crate::permissions::is_microphone_authorized()
}

/// Request microphone permission from the system
/// On macOS, this triggers the native permission dialog
/// Returns true if permission was granted
#[tauri::command]
pub async fn request_microphone_permission(state: State<'_, AppState>) -> Result<bool, String> {
    tracing::info!("Requesting microphone permission");

    // Run in blocking task since it waits for user response
    let granted = tokio::task::spawn_blocking(crate::permissions::request_microphone_permission)
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

/// Get list of available models on disk (bundled models only)
#[tauri::command]
pub fn get_available_models(app: AppHandle) -> Result<Vec<String>, String> {
    let models_dir = get_models_dir(&app)?;
    tracing::info!("Checking models in: {}", models_dir.display());

    let mut available = Vec::new();

    // Only check bundled models: small and large-v3-turbo
    let model_ids = ["small", "large-v3-turbo"];

    for model_id in model_ids {
        let filename = format!("ggml-{}-q5_0.bin", model_id);
        let path = models_dir.join(&filename);
        if path.exists() {
            tracing::info!("Found model: {}", model_id);
            available.push(model_id.to_string());
        }
    }

    Ok(available)
}
