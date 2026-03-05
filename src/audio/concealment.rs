//! Packet loss concealment strategies
//!
//! When packets are lost, we need to fill the gap to maintain
//! continuous audio output.

/// Concealment strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcealmentStrategy {
    /// Fill with silence (zeros)
    Silence,
    /// Repeat the previous packet
    #[default]
    Repeat,
    /// Fade to silence over the gap
    FadeOut,
    /// Interpolate if next packet arrives
    Interpolate,
}

/// Concealment generator
pub struct Concealer {
    strategy: ConcealmentStrategy,
    /// Previous packet audio data (for repeat)
    previous_audio: Option<Vec<u8>>,
    /// Sample rate
    #[allow(dead_code)] // Will be used for more advanced concealment
    sample_rate: u32,
    /// Bytes per sample (e.g., 4 for 16-bit stereo)
    bytes_per_sample: usize,
}

impl Concealer {
    /// Create a new concealer
    #[must_use]
    pub fn new(strategy: ConcealmentStrategy, sample_rate: u32, bytes_per_sample: usize) -> Self {
        Self {
            strategy,
            previous_audio: None,
            sample_rate,
            bytes_per_sample,
        }
    }

    /// Record a good packet for later concealment
    pub fn record_good_packet(&mut self, audio: &[u8]) {
        self.previous_audio = Some(audio.to_vec());
    }

    /// Generate concealment audio for a missing packet
    #[must_use]
    pub fn conceal(&self, packet_samples: usize) -> Vec<u8> {
        let size = packet_samples * self.bytes_per_sample;

        match self.strategy {
            ConcealmentStrategy::Silence => {
                vec![0u8; size]
            }
            ConcealmentStrategy::Repeat => self
                .previous_audio
                .clone()
                .unwrap_or_else(|| vec![0u8; size]),
            ConcealmentStrategy::FadeOut => {
                // Fade previous packet to silence
                if let Some(ref prev) = self.previous_audio {
                    self.fade_out(prev, size)
                } else {
                    vec![0u8; size]
                }
            }
            ConcealmentStrategy::Interpolate => {
                // Would need next packet; fall back to repeat
                self.previous_audio
                    .clone()
                    .unwrap_or_else(|| vec![0u8; size])
            }
        }
    }

    /// Fade audio to silence
    fn fade_out(&self, audio: &[u8], target_size: usize) -> Vec<u8> {
        let mut output = audio.to_vec();
        output.resize(target_size, 0);

        // Calculate number of frames based on bytes_per_sample
        // For 16-bit stereo (4 bytes), each frame has 2 samples
        let frame_count = output.len() / self.bytes_per_sample;

        // Process one frame at a time
        for i in 0..frame_count {
            #[allow(
                clippy::cast_precision_loss,
                reason = "Precision loss is acceptable for audio fade calculation"
            )]
            let fade = 1.0 - (i as f32 / frame_count as f32);

            let frame_start = i * self.bytes_per_sample;
            let frame_end = frame_start + self.bytes_per_sample;

            // Apply fade to all samples in this frame
            // Assuming 16-bit samples (2 bytes each)
            for sample_idx in (frame_start..frame_end).step_by(2) {
                if sample_idx + 1 < output.len() {
                    let sample = i16::from_le_bytes([output[sample_idx], output[sample_idx + 1]]);
                    #[allow(clippy::cast_possible_truncation, reason = "Audio sample fits in i16")]
                    let faded = (f32::from(sample) * fade) as i16;
                    let bytes = faded.to_le_bytes();
                    output[sample_idx] = bytes[0];
                    output[sample_idx + 1] = bytes[1];
                }
            }
        }

        output
    }
}
