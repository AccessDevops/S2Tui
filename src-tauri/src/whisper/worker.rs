use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::available_parallelism;
use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::whisper::GpuBackend;

/// Calculate optimal thread count: 75% of available CPUs, minimum 1
/// Leaves headroom for UI responsiveness and system tasks
fn optimal_thread_count() -> i32 {
    let cpus = available_parallelism().map(|p| p.get()).unwrap_or(4); // Fallback to 4 if detection fails

    let threads = ((cpus as f32) * 0.75).ceil() as i32;
    threads.max(1) // At least 1 thread
}

#[derive(Error, Debug)]
pub enum WhisperError {
    #[error("Model not loaded")]
    NotLoaded,
    #[error("Failed to load model: {0}")]
    LoadError(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),
    #[error("Invalid audio data")]
    InvalidAudio,
}

/// Résultat du chargement du modèle
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLoadResult {
    /// Chargement réussi
    pub success: bool,
    /// GPU utilisé pour le modèle
    pub using_gpu: bool,
    /// Backend utilisé (nom)
    pub backend: String,
    /// Fallback CPU utilisé après échec GPU
    pub fallback_used: bool,
}

#[derive(Debug, Clone)]
pub struct WhisperConfig {
    pub model_path: PathBuf,
    pub language: Option<String>,
    pub translate: bool,
    pub n_threads: i32,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        let threads = optimal_thread_count();
        tracing::info!("Whisper using {} threads (75% of available CPUs)", threads);
        Self {
            model_path: PathBuf::new(),
            language: None, // Auto-detect
            translate: false,
            n_threads: threads,
        }
    }
}

/// Whisper transcription engine using whisper-rs native bindings
pub struct WhisperEngine {
    context: Option<WhisperContext>,
    config: WhisperConfig,
    /// Track if GPU is being used for transcription
    using_gpu: bool,
    /// Track if fallback to CPU was used
    fallback_used: bool,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            context: None,
            config: WhisperConfig::default(),
            using_gpu: false,
            fallback_used: false,
        }
    }

    /// Check if GPU is being used
    pub fn is_using_gpu(&self) -> bool {
        self.using_gpu
    }

    /// Check if fallback to CPU was used
    pub fn was_fallback_used(&self) -> bool {
        self.fallback_used
    }

    /// Get the current backend name
    pub fn get_backend_name(&self) -> String {
        if self.using_gpu {
            crate::whisper::detect_active_backend().name().to_string()
        } else {
            "CPU".to_string()
        }
    }

    /// Load a model from the given path (legacy method, uses GPU if available)
    pub fn load_model(&mut self, model_path: PathBuf) -> Result<(), WhisperError> {
        self.load_model_with_options(model_path, false).map(|_| ())
    }

    /// Load a model with explicit CPU/GPU control
    /// Returns ModelLoadResult with details about the loading
    pub fn load_model_with_options(
        &mut self,
        model_path: PathBuf,
        force_cpu: bool,
    ) -> Result<ModelLoadResult, WhisperError> {
        if !model_path.exists() {
            return Err(WhisperError::ModelNotFound(
                model_path.display().to_string(),
            ));
        }

        // Detect GPU backend
        let gpu_backend = crate::whisper::detect_active_backend();
        let should_use_gpu = gpu_backend != GpuBackend::Cpu && !force_cpu;

        tracing::info!(
            "Loading Whisper model: {} (force_cpu={}, detected_backend={:?})",
            model_path.display(),
            force_cpu,
            gpu_backend
        );

        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| WhisperError::LoadError("Invalid model path".to_string()))?;

        // First attempt: with GPU if available and not forced CPU
        if should_use_gpu {
            tracing::info!(
                "Attempting to load model with GPU ({})...",
                gpu_backend.name()
            );

            let mut params = WhisperContextParameters::default();
            params.use_gpu(true);

            match WhisperContext::new_with_params(model_path_str, params) {
                Ok(ctx) => {
                    self.context = Some(ctx);
                    self.config.model_path = model_path;
                    self.using_gpu = true;
                    self.fallback_used = false;

                    tracing::info!(
                        "Whisper model loaded successfully with {} GPU acceleration",
                        gpu_backend.name()
                    );

                    return Ok(ModelLoadResult {
                        success: true,
                        using_gpu: true,
                        backend: gpu_backend.name().to_string(),
                        fallback_used: false,
                    });
                }
                Err(gpu_error) => {
                    tracing::warn!(
                        "GPU loading failed: {}. Retrying with CPU fallback...",
                        gpu_error
                    );

                    // Fall through to CPU attempt
                }
            }
        }

        // CPU attempt (either forced or as fallback)
        tracing::info!("Loading model with CPU...");

        let mut cpu_params = WhisperContextParameters::default();
        cpu_params.use_gpu(false);

        let ctx = WhisperContext::new_with_params(model_path_str, cpu_params)
            .map_err(|e| WhisperError::LoadError(format!("CPU loading failed: {}", e)))?;

        self.context = Some(ctx);
        self.config.model_path = model_path;
        self.using_gpu = false;
        self.fallback_used = should_use_gpu; // True if we tried GPU first and failed

        if self.fallback_used {
            tracing::info!("Whisper model loaded with CPU (fallback from GPU failure)");
        } else {
            tracing::info!("Whisper model loaded with CPU (as requested)");
        }

        Ok(ModelLoadResult {
            success: true,
            using_gpu: false,
            backend: "CPU".to_string(),
            fallback_used: self.fallback_used,
        })
    }

    /// Set the language for transcription (None for auto-detect)
    pub fn set_language(&mut self, language: Option<String>) {
        self.config.language = language;
    }

    /// Check if a model is loaded
    pub fn is_loaded(&self) -> bool {
        self.context.is_some()
    }

    /// Transcribe audio samples (i16 PCM, 16kHz mono)
    pub fn transcribe(&self, samples: &[i16]) -> Result<String, WhisperError> {
        let ctx = self.context.as_ref().ok_or(WhisperError::NotLoaded)?;

        if samples.is_empty() {
            return Err(WhisperError::InvalidAudio);
        }

        // Convert i16 samples to f32 (whisper-rs expects f32)
        let samples_f32: Vec<f32> = samples
            .iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect();

        tracing::info!(
            "Transcribing {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f32 / 16000.0
        );

        // Create transcription parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Set language
        if let Some(ref lang) = self.config.language {
            params.set_language(Some(lang));
        } else {
            params.set_language(None); // Auto-detect
        }

        params.set_translate(self.config.translate);
        params.set_n_threads(self.config.n_threads);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Create a new state for this transcription
        let mut state = ctx.create_state().map_err(|e| {
            WhisperError::TranscriptionError(format!("Failed to create state: {}", e))
        })?;

        // Run transcription
        state.full(params, &samples_f32).map_err(|e| {
            WhisperError::TranscriptionError(format!("Transcription failed: {}", e))
        })?;

        // Get the transcription result
        let num_segments = state.full_n_segments().map_err(|e| {
            WhisperError::TranscriptionError(format!("Failed to get segments: {}", e))
        })?;

        let mut result = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                result.push_str(&segment);
                result.push(' ');
            }
        }

        let result = result.trim().to_string();
        tracing::info!("Transcription complete: \"{}\"", result);

        Ok(result)
    }
}

impl Default for WhisperEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for WhisperEngine
pub struct WhisperWorker {
    engine: Arc<Mutex<WhisperEngine>>,
}

impl WhisperWorker {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(Mutex::new(WhisperEngine::new())),
        }
    }

    /// Load a model (thread-safe)
    pub fn load_model(&self, model_path: PathBuf) -> Result<(), WhisperError> {
        self.engine.lock().load_model(model_path)
    }

    /// Load a model with explicit CPU/GPU control (thread-safe)
    pub fn load_model_with_options(
        &self,
        model_path: PathBuf,
        force_cpu: bool,
    ) -> Result<ModelLoadResult, WhisperError> {
        self.engine
            .lock()
            .load_model_with_options(model_path, force_cpu)
    }

    /// Set language (thread-safe)
    pub fn set_language(&self, language: Option<String>) {
        self.engine.lock().set_language(language);
    }

    /// Check if model is loaded (thread-safe)
    pub fn is_loaded(&self) -> bool {
        self.engine.lock().is_loaded()
    }

    /// Check if GPU is being used (thread-safe)
    pub fn is_using_gpu(&self) -> bool {
        self.engine.lock().is_using_gpu()
    }

    /// Check if fallback was used (thread-safe)
    pub fn was_fallback_used(&self) -> bool {
        self.engine.lock().was_fallback_used()
    }

    /// Get current backend name (thread-safe)
    pub fn get_backend_name(&self) -> String {
        self.engine.lock().get_backend_name()
    }

    /// Transcribe samples (thread-safe)
    pub fn transcribe(&self, samples: &[i16]) -> Result<String, WhisperError> {
        self.engine.lock().transcribe(samples)
    }
}

impl Default for WhisperWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WhisperWorker {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = WhisperConfig::default();
        assert!(config.language.is_none());
        assert!(!config.translate);
        // n_threads is now dynamic (75% of CPUs), just ensure it's at least 1
        assert!(config.n_threads >= 1);
    }

    #[test]
    fn test_engine_not_loaded() {
        let engine = WhisperEngine::new();
        assert!(!engine.is_loaded());

        let result = engine.transcribe(&[0i16; 1000]);
        assert!(matches!(result, Err(WhisperError::NotLoaded)));
    }
}
