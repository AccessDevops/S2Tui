use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::available_parallelism;
use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Calculate optimal thread count: 75% of available CPUs, minimum 1
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
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            context: None,
            config: WhisperConfig::default(),
        }
    }

    /// Load a model from the given path
    pub fn load_model(&mut self, model_path: PathBuf) -> Result<(), WhisperError> {
        if !model_path.exists() {
            return Err(WhisperError::ModelNotFound(
                model_path.display().to_string(),
            ));
        }

        tracing::info!("Loading Whisper model: {}", model_path.display());

        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| WhisperError::LoadError("Invalid model path".to_string()))?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| WhisperError::LoadError(e.to_string()))?;

        self.context = Some(ctx);
        self.config.model_path = model_path;

        tracing::info!("Whisper model loaded successfully");
        Ok(())
    }

    /// Set the language for transcription (None for auto-detect)
    pub fn set_language(&mut self, language: Option<String>) {
        self.config.language = language;
    }

    /// Set whether to translate to English
    #[allow(dead_code)]
    pub fn set_translate(&mut self, translate: bool) {
        self.config.translate = translate;
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

    /// Set language (thread-safe)
    pub fn set_language(&self, language: Option<String>) {
        self.engine.lock().set_language(language);
    }

    /// Check if model is loaded (thread-safe)
    pub fn is_loaded(&self) -> bool {
        self.engine.lock().is_loaded()
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
