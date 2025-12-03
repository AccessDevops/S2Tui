use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Error, Debug)]
pub enum AudioCaptureError {
    #[error("Failed to initialize audio device: {0}")]
    DeviceError(String),
    #[error("Failed to build audio stream: {0}")]
    StreamError(String),
    #[error("Audio capture not available")]
    NotAvailable,
    #[error("No input device found")]
    NoInputDevice,
    #[error("Unsupported sample format")]
    UnsupportedFormat,
}

/// Audio buffer for storing captured samples
pub struct AudioBuffer {
    samples: Vec<i16>,
    sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            samples: Vec::with_capacity(sample_rate as usize * 30), // 30 seconds buffer
            sample_rate,
        }
    }

    pub fn push(&mut self, samples: &[i16]) {
        self.samples.extend_from_slice(samples);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn get_samples(&self) -> &[i16] {
        &self.samples
    }

    pub fn take_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.samples)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// Audio chunk sent from capture thread
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

/// Audio capture handler using cpal
pub struct AudioCapture {
    buffer: Arc<Mutex<AudioBuffer>>,
    is_capturing: Arc<AtomicBool>,
    stream: Mutex<Option<Stream>>,
    chunk_sender: Mutex<Option<mpsc::UnboundedSender<AudioChunk>>>,
    target_sample_rate: u32,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(AudioBuffer::new(16000))), // 16kHz for Whisper
            is_capturing: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
            chunk_sender: Mutex::new(None),
            target_sample_rate: 16000, // Whisper expects 16kHz
        }
    }

    /// Create a channel to receive audio chunks
    pub fn create_chunk_channel(&self) -> mpsc::UnboundedReceiver<AudioChunk> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.chunk_sender.lock() = Some(tx);
        rx
    }

    /// Start capturing audio from the default input device
    pub fn start(&self) -> Result<(), AudioCaptureError> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(()); // Already capturing
        }

        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(AudioCaptureError::NoInputDevice)?;

        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        tracing::info!("Using input device: {}", device_name);

        let config = device
            .default_input_config()
            .map_err(|e| AudioCaptureError::DeviceError(e.to_string()))?;

        let source_sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        tracing::info!(
            "Input config: {} Hz, {} channels, format: {:?}",
            source_sample_rate,
            channels,
            config.sample_format()
        );

        let buffer = Arc::clone(&self.buffer);
        let is_capturing = Arc::clone(&self.is_capturing);
        let chunk_sender = self.chunk_sender.lock().clone();
        let target_rate = self.target_sample_rate;

        // Resampling state
        let resample_ratio = target_rate as f64 / source_sample_rate as f64;

        let err_fn = |err| tracing::error!("Audio stream error: {}", err);

        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                let stream = device
                    .build_input_stream(
                        &config.into(),
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            if !is_capturing.load(Ordering::SeqCst) {
                                return;
                            }

                            // Convert f32 to i16 and handle channels
                            let mono_samples: Vec<i16> = data
                                .chunks(channels)
                                .map(|frame| {
                                    // Average channels for mono
                                    let sum: f32 = frame.iter().sum();
                                    let avg = sum / channels as f32;
                                    (avg * i16::MAX as f32) as i16
                                })
                                .collect();

                            // Simple resampling (linear interpolation)
                            let resampled = resample(&mono_samples, resample_ratio);

                            // Store in buffer
                            buffer.lock().push(&resampled);

                            // Send chunk for real-time processing
                            if let Some(ref sender) = chunk_sender {
                                let _ = sender.send(AudioChunk {
                                    samples: resampled,
                                    sample_rate: target_rate,
                                });
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| AudioCaptureError::StreamError(e.to_string()))?;
                stream
            }
            SampleFormat::I16 => {
                let stream = device
                    .build_input_stream(
                        &config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            if !is_capturing.load(Ordering::SeqCst) {
                                return;
                            }

                            // Handle channels
                            let mono_samples: Vec<i16> = data
                                .chunks(channels)
                                .map(|frame| {
                                    let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                                    (sum / channels as i32) as i16
                                })
                                .collect();

                            // Simple resampling
                            let resampled = resample(&mono_samples, resample_ratio);

                            buffer.lock().push(&resampled);

                            if let Some(ref sender) = chunk_sender {
                                let _ = sender.send(AudioChunk {
                                    samples: resampled,
                                    sample_rate: target_rate,
                                });
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| AudioCaptureError::StreamError(e.to_string()))?;
                stream
            }
            _ => return Err(AudioCaptureError::UnsupportedFormat),
        };

        stream
            .play()
            .map_err(|e| AudioCaptureError::StreamError(e.to_string()))?;

        self.is_capturing.store(true, Ordering::SeqCst);
        *self.stream.lock() = Some(stream);

        tracing::info!("Audio capture started");
        Ok(())
    }

    /// Stop capturing audio and return all captured samples
    pub fn stop(&self) -> Result<Vec<i16>, AudioCaptureError> {
        self.is_capturing.store(false, Ordering::SeqCst);

        // Drop the stream to stop it
        if let Some(stream) = self.stream.lock().take() {
            drop(stream);
        }

        let samples = self.buffer.lock().take_samples();
        tracing::info!(
            "Audio capture stopped, {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f32 / self.target_sample_rate as f32
        );

        Ok(samples)
    }

    /// Check if currently capturing
    pub fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }

    /// Get current buffer duration
    pub fn buffer_duration(&self) -> f32 {
        self.buffer.lock().duration_secs()
    }

    /// Clear the buffer
    pub fn clear_buffer(&self) {
        self.buffer.lock().clear();
    }

    /// Get the target sample rate
    pub fn sample_rate(&self) -> u32 {
        self.target_sample_rate
    }
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple linear interpolation resampling
fn resample(samples: &[i16], ratio: f64) -> Vec<i16> {
    if (ratio - 1.0).abs() < 0.001 {
        return samples.to_vec();
    }

    let output_len = (samples.len() as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 / ratio;
        let src_idx_floor = src_idx.floor() as usize;
        let src_idx_ceil = (src_idx_floor + 1).min(samples.len() - 1);
        let frac = src_idx - src_idx_floor as f64;

        if src_idx_floor >= samples.len() {
            break;
        }

        let sample = if src_idx_ceil < samples.len() {
            let s0 = samples[src_idx_floor] as f64;
            let s1 = samples[src_idx_ceil] as f64;
            (s0 * (1.0 - frac) + s1 * frac) as i16
        } else {
            samples[src_idx_floor]
        };

        output.push(sample);
    }

    output
}

// Ensure Stream is Send (it's not by default with some backends)
// We handle this by wrapping in Mutex
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}
