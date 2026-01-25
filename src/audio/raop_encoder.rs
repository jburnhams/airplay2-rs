//! RAOP audio encoding with encryption

use crate::protocol::raop::encryption::RaopEncryptor;

/// RAOP audio encoder with encryption
pub struct RaopAudioEncoder {
    /// Audio encryptor
    encryptor: RaopEncryptor,
    /// Current packet index
    packet_index: u64,
    /// Sample rate
    sample_rate: u32,
    /// Samples per frame
    samples_per_frame: u32,
}

impl RaopAudioEncoder {
    /// Samples per ALAC frame
    pub const ALAC_FRAME_SAMPLES: u32 = 352;

    /// Create new encoder
    #[must_use]
    pub fn new(encryptor: RaopEncryptor) -> Self {
        Self {
            encryptor,
            packet_index: 0,
            sample_rate: 44100,
            samples_per_frame: Self::ALAC_FRAME_SAMPLES,
        }
    }

    /// Encode and encrypt a frame of audio
    ///
    /// # Arguments
    /// * `pcm_samples` - 16-bit stereo PCM samples (interleaved L/R)
    ///
    /// # Returns
    /// Encrypted audio data ready for RTP packet
    ///
    /// # Errors
    /// Returns `AudioEncodeError` if encryption or encoding fails
    pub fn encode_frame(&mut self, pcm_samples: &[i16]) -> Result<Vec<u8>, AudioEncodeError> {
        // Convert to bytes
        let mut audio_bytes = Vec::with_capacity(pcm_samples.len() * 2);
        for sample in pcm_samples {
            audio_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        // For PCM mode, use raw bytes
        // For ALAC mode, would encode here (placeholder)

        // Encrypt
        let encrypted = self
            .encryptor
            .encrypt(&audio_bytes, self.packet_index)
            .map_err(|e| AudioEncodeError::Encryption(e.to_string()))?;

        self.packet_index += 1;

        Ok(encrypted)
    }

    /// Encode raw audio bytes
    ///
    /// # Errors
    /// Returns `AudioEncodeError` if encryption fails
    pub fn encode_raw(&mut self, audio_data: &[u8]) -> Result<Vec<u8>, AudioEncodeError> {
        let encrypted = self
            .encryptor
            .encrypt(audio_data, self.packet_index)
            .map_err(|e| AudioEncodeError::Encryption(e.to_string()))?;

        self.packet_index += 1;

        Ok(encrypted)
    }

    /// Reset packet index (after flush)
    pub fn reset(&mut self) {
        self.packet_index = 0;
    }

    /// Get current packet index
    #[must_use]
    pub fn packet_index(&self) -> u64 {
        self.packet_index
    }
}

/// Audio encoding errors
#[derive(Debug, thiserror::Error)]
pub enum AudioEncodeError {
    /// Encryption failed
    #[error("encryption error: {0}")]
    Encryption(String),
    /// Frame size mismatch
    #[error("invalid frame size: expected {expected}, got {actual}")]
    InvalidFrameSize {
        /// Expected size in bytes
        expected: usize,
        /// Actual size in bytes
        actual: usize,
    },
    /// Encoding failed
    #[error("encoding error: {0}")]
    Encoding(String),
}
