use thiserror::Error;

#[derive(Error, Debug)]
pub enum VadError {
    #[error("VAD initialization failed: {0}")]
    InitError(String),
    #[error("VAD processing failed: {0}")]
    ProcessError(String),
}

/// Voice Activity Detection result
#[derive(Debug, Clone, Copy)]
pub struct VadResult {
    /// Is speech detected
    pub is_speech: bool,
    /// RMS level (0.0 - 1.0)
    pub rms_level: f32,
}

/// Voice Activity Detector
pub struct VoiceActivityDetector {
    /// Threshold for speech detection
    speech_threshold: f32,
    /// Minimum silence duration before stopping (in frames)
    silence_frames_threshold: usize,
    /// Current silence frame count
    silence_frames: usize,
    /// Is currently in speech segment
    in_speech: bool,
}

impl VoiceActivityDetector {
    pub fn new() -> Self {
        Self {
            speech_threshold: 0.02, // Adjust based on testing
            silence_frames_threshold: 15, // ~1.5 seconds at 10fps
            silence_frames: 0,
            in_speech: false,
        }
    }

    /// Process audio samples and detect voice activity
    pub fn process(&mut self, samples: &[i16]) -> VadResult {
        let rms = self.calculate_rms(samples);
        let is_speech = rms > self.speech_threshold;

        if is_speech {
            self.silence_frames = 0;
            self.in_speech = true;
        } else if self.in_speech {
            self.silence_frames += 1;
            if self.silence_frames >= self.silence_frames_threshold {
                self.in_speech = false;
            }
        }

        // Use logarithmic scaling for better visual representation
        // Human perception of loudness is logarithmic
        let display_level = if rms > 0.001 {
            // Convert to dB-like scale, then normalize to 0-1
            // rms of 0.01 -> ~0.3, rms of 0.1 -> ~0.7, rms of 0.3 -> ~1.0
            let db = 20.0 * rms.log10();
            let normalized = (db + 40.0) / 30.0; // -40dB to -10dB range -> 0 to 1
            normalized.clamp(0.0, 1.0)
        } else {
            0.0
        };

        VadResult {
            is_speech: self.in_speech,
            rms_level: display_level,
        }
    }

    /// Calculate RMS (Root Mean Square) of samples
    fn calculate_rms(&self, samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum: f64 = samples
            .iter()
            .map(|&s| {
                let normalized = s as f64 / i16::MAX as f64;
                normalized * normalized
            })
            .sum();

        (sum / samples.len() as f64).sqrt() as f32
    }

    /// Reset the VAD state
    pub fn reset(&mut self) {
        self.silence_frames = 0;
        self.in_speech = false;
    }

    /// Check if speech has ended (silence timeout)
    pub fn speech_ended(&self) -> bool {
        !self.in_speech && self.silence_frames >= self.silence_frames_threshold
    }

    /// Set speech detection threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.speech_threshold = threshold.clamp(0.001, 0.5);
    }

    /// Set silence timeout in frames
    pub fn set_silence_timeout(&mut self, frames: usize) {
        self.silence_frames_threshold = frames.max(1);
    }
}

impl Default for VoiceActivityDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_calculation() {
        let vad = VoiceActivityDetector::new();

        // Silent samples
        let silent: Vec<i16> = vec![0; 1000];
        assert!(vad.calculate_rms(&silent) < 0.001);

        // Max amplitude samples
        let loud: Vec<i16> = vec![i16::MAX; 1000];
        assert!(vad.calculate_rms(&loud) > 0.9);
    }

    #[test]
    fn test_speech_detection() {
        let mut vad = VoiceActivityDetector::new();

        // Silent should not trigger speech
        let silent: Vec<i16> = vec![0; 1000];
        let result = vad.process(&silent);
        assert!(!result.is_speech);

        // Loud should trigger speech
        let loud: Vec<i16> = vec![5000; 1000];
        let result = vad.process(&loud);
        assert!(result.is_speech);
    }
}
