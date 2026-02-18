//! RTP Audio Decryption for `AirPlay` 2
//!
//! Decrypts RTP audio payloads using ChaCha20-Poly1305 AEAD.

use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};
use crate::protocol::rtp::RtpPacket;

/// `AirPlay` 2 RTP decryptor
pub struct Ap2RtpDecryptor {
    /// Decryption key (from SETUP)
    key: [u8; 32],
    /// AAD (Additional Authenticated Data) prefix
    aad_prefix: Option<Vec<u8>>,
}

impl Ap2RtpDecryptor {
    /// Create a new decryptor with the shared key from SETUP
    #[must_use]
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            aad_prefix: None,
        }
    }

    /// Set AAD prefix (if required by stream configuration)
    pub fn set_aad_prefix(&mut self, prefix: Vec<u8>) {
        self.aad_prefix = Some(prefix);
    }

    /// Decrypt an RTP packet payload
    ///
    /// # Arguments
    /// * `packet` - The RTP packet with encrypted payload
    ///
    /// # Returns
    /// Decrypted audio data
    ///
    /// # Errors
    /// Returns `DecryptionError` if payload is too short or authentication fails.
    pub fn decrypt(&self, packet: &RtpPacket) -> Result<Vec<u8>, DecryptionError> {
        let payload = &packet.payload;

        if payload.len() < 16 {
            return Err(DecryptionError::PayloadTooShort);
        }

        // Build nonce from RTP header
        let nonce = Self::build_nonce(packet);

        // Build AAD if configured
        let aad = self.build_aad(packet);

        // Decrypt with AEAD
        let cipher = ChaCha20Poly1305Cipher::new(&self.key)
            .map_err(|_| DecryptionError::AuthenticationFailed)?;

        let nonce_obj =
            Nonce::from_bytes(&nonce).map_err(|_| DecryptionError::AuthenticationFailed)?;

        let plaintext = if let Some(ref aad) = aad {
            cipher.decrypt_with_aad(&nonce_obj, aad, payload)
        } else {
            cipher.decrypt(&nonce_obj, payload)
        }
        .map_err(|_| DecryptionError::AuthenticationFailed)?;

        Ok(plaintext)
    }

    /// Build nonce from RTP header fields
    ///
    /// Nonce format for `AirPlay` 2:
    /// - 4 bytes: zeros
    /// - 4 bytes: SSRC (big-endian)
    /// - 4 bytes: sequence + timestamp bits
    fn build_nonce(packet: &RtpPacket) -> [u8; 12] {
        let mut nonce = [0u8; 12];

        // SSRC at offset 4
        nonce[4..8].copy_from_slice(&packet.header.ssrc.to_be_bytes());

        // Sequence at offset 8 (extended to 4 bytes)
        nonce[8..10].copy_from_slice(&packet.header.sequence.to_be_bytes());

        // Could include timestamp bits in remaining bytes
        // This varies by implementation

        nonce
    }

    /// Build AAD from RTP header
    fn build_aad(&self, packet: &RtpPacket) -> Option<Vec<u8>> {
        self.aad_prefix.as_ref().map(|prefix| {
            let mut aad = prefix.clone();
            // Add RTP header bytes
            aad.extend_from_slice(&packet.header.encode());
            aad
        })
    }
}

/// Errors occurring during RTP decryption
#[derive(Debug, thiserror::Error)]
pub enum DecryptionError {
    /// Payload is too short to contain authentication tag
    #[error("Payload too short (< 16 bytes for auth tag)")]
    PayloadTooShort,

    /// Authentication failed (integrity check)
    #[error("Authentication failed - corrupted or tampered packet")]
    AuthenticationFailed,
}

/// Audio format handler
pub trait AudioDecoder: Send + Sync {
    /// Decode audio data to PCM samples
    ///
    /// # Errors
    /// Returns `AudioDecodeError` if data is invalid or format unsupported.
    fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>, AudioDecodeError>;

    /// Get sample rate
    fn sample_rate(&self) -> u32;

    /// Get channel count
    fn channels(&self) -> u8;
}

/// Audio decoding errors
#[derive(Debug, thiserror::Error)]
pub enum AudioDecodeError {
    /// Invalid audio data
    #[error("Invalid audio data")]
    InvalidData,

    /// Unsupported audio format or parameters
    #[error("Unsupported format")]
    UnsupportedFormat,

    /// Internal decoder error
    #[error("Decoder error: {0}")]
    DecoderError(String),
}

/// PCM passthrough decoder
pub struct PcmDecoder {
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
}

impl PcmDecoder {
    /// Create a new PCM decoder
    #[must_use]
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8) -> Self {
        Self {
            sample_rate,
            channels,
            bits_per_sample,
        }
    }
}

impl AudioDecoder for PcmDecoder {
    fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>, AudioDecodeError> {
        match self.bits_per_sample {
            16 => {
                // 16-bit signed LE samples
                let samples: Vec<i16> = data
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();
                Ok(samples)
            }
            24 => {
                // 24-bit to 16-bit conversion
                // AirPlay 2 24-bit is usually packed as 24-bit little endian.
                // We convert to 16-bit by taking the upper 16 bits.
                let samples: Vec<i16> = data
                    .chunks_exact(3)
                    .map(|chunk| {
                        // Construct 32-bit int with sample in upper 24 bits: [0, LSB, Mid, MSB]
                        // This ensures correct sign extension when shifting down.
                        let value = i32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]);
                        // Shift right by 16 to get top 16 bits (dropping bottom 8 bits of 24-bit
                        // sample)
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            (value >> 16) as i16
                        }
                    })
                    .collect();
                Ok(samples)
            }
            _ => Err(AudioDecodeError::UnsupportedFormat),
        }
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn channels(&self) -> u8 {
        self.channels
    }
}

/// ALAC decoder wrapper
pub struct AlacDecoder {
    // Would wrap alac-decoder crate
    sample_rate: u32,
    channels: u8,
}

impl AlacDecoder {
    /// Create a new ALAC decoder
    ///
    /// # Errors
    /// Returns `AudioDecodeError` if initialization fails.
    pub fn new(
        sample_rate: u32,
        channels: u8,
        _magic_cookie: &[u8],
    ) -> Result<Self, AudioDecodeError> {
        // Initialize ALAC decoder with magic cookie
        Ok(Self {
            sample_rate,
            channels,
        })
    }
}

impl AudioDecoder for AlacDecoder {
    fn decode(&mut self, _data: &[u8]) -> Result<Vec<i16>, AudioDecodeError> {
        // Would call into alac-decoder crate
        // For now, placeholder
        Err(AudioDecodeError::DecoderError(
            "ALAC decoder not implemented".into(),
        ))
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn channels(&self) -> u8 {
        self.channels
    }
}
