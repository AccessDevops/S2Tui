use std::collections::VecDeque;

/// Audio streamer for sending audio chunks to the Whisper worker
pub struct AudioStreamer {
    /// Buffer of audio chunks
    buffer: VecDeque<Vec<i16>>,
    /// Chunk size in samples
    chunk_size: usize,
    /// Sample rate
    sample_rate: u32,
}

impl AudioStreamer {
    pub fn new(sample_rate: u32) -> Self {
        // 100ms chunks at given sample rate
        let chunk_size = (sample_rate as usize) / 10;

        Self {
            buffer: VecDeque::new(),
            chunk_size,
            sample_rate,
        }
    }

    /// Add samples to the buffer
    pub fn push(&mut self, samples: &[i16]) {
        // Split samples into chunks
        for chunk in samples.chunks(self.chunk_size) {
            self.buffer.push_back(chunk.to_vec());
        }
    }

    /// Get the next chunk of audio
    pub fn pop(&mut self) -> Option<Vec<i16>> {
        self.buffer.pop_front()
    }

    /// Get all buffered audio as a single vector
    pub fn drain(&mut self) -> Vec<i16> {
        let total_samples: usize = self.buffer.iter().map(|c| c.len()).sum();
        let mut result = Vec::with_capacity(total_samples);

        while let Some(chunk) = self.buffer.pop_front() {
            result.extend(chunk);
        }

        result
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get number of buffered chunks
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Get total duration of buffered audio in seconds
    pub fn duration_secs(&self) -> f32 {
        let total_samples: usize = self.buffer.iter().map(|c| c.len()).sum();
        total_samples as f32 / self.sample_rate as f32
    }
}

impl Default for AudioStreamer {
    fn default() -> Self {
        Self::new(16000) // Default to 16kHz for Whisper
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_drain() {
        let mut streamer = AudioStreamer::new(16000);
        let samples: Vec<i16> = (0..3200).map(|i| i as i16).collect();

        streamer.push(&samples);
        assert!(!streamer.is_empty());

        let drained = streamer.drain();
        assert_eq!(drained.len(), 3200);
        assert!(streamer.is_empty());
    }

    #[test]
    fn test_duration() {
        let mut streamer = AudioStreamer::new(16000);
        let samples: Vec<i16> = vec![0; 16000]; // 1 second of audio

        streamer.push(&samples);
        assert!((streamer.duration_secs() - 1.0).abs() < 0.01);
    }
}
