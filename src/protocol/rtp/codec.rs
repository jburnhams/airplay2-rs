use super::packet::{RtpDecodeError, RtpHeader, RtpPacket};
use crate::protocol::crypto::{Aes128Ctr, ChaCha20Poly1305Cipher, Nonce};
use thiserror::Error;

/// RTP codec errors
#[derive(Debug, Error)]
pub enum RtpCodecError {
    #[error("decode error: {0}")]
    Decode(#[from] RtpDecodeError),

    #[error("encryption not initialized")]
    EncryptionNotInitialized,

    #[error("invalid audio data size: {0} bytes")]
    InvalidAudioSize(usize),

    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
}

/// Encryption mode for RTP packets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpEncryptionMode {
    /// No encryption
    None,
    /// AES-128-CTR (legacy `AirPlay` 1)
    Aes128Ctr,
    /// ChaCha20-Poly1305 (`AirPlay` 2)
    ChaCha20Poly1305,
}

/// RTP codec for encoding/decoding audio packets
///
/// Handles encryption if keys are set.
pub struct RtpCodec {
    /// SSRC for outgoing packets
    ssrc: u32,
    /// Current sequence number
    sequence: u16,
    /// Current RTP timestamp
    timestamp: u32,
    /// AES key for encryption (None = unencrypted)
    aes_key: Option<[u8; 16]>,
    /// AES IV for encryption
    aes_iv: Option<[u8; 16]>,
    /// ChaCha20-Poly1305 key (32 bytes)
    chacha_key: Option<[u8; 32]>,
    /// Encryption mode
    encryption_mode: RtpEncryptionMode,
    /// Use buffered audio mode
    buffered_mode: bool,
    /// Nonce counter for ChaCha20-Poly1305
    nonce_counter: u64,
    /// Frames per packet (default 352)
    frames_per_packet: u32,
}

impl RtpCodec {
    /// Samples per packet (default)
    pub const FRAMES_PER_PACKET: u32 = 352;

    /// Poly1305 tag size
    pub const TAG_SIZE: usize = 16;

    /// Nonce size for ChaCha20-Poly1305 (8 bytes sent in packet, 12 bytes total with padding)
    pub const NONCE_SIZE: usize = 8;

    /// Create a new codec
    #[must_use]
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            sequence: 0,
            timestamp: 0,
            aes_key: None,
            aes_iv: None,
            chacha_key: None,
            encryption_mode: RtpEncryptionMode::None,
            buffered_mode: false,
            nonce_counter: 0,
            frames_per_packet: Self::FRAMES_PER_PACKET,
        }
    }

    /// Set frames per packet
    pub fn set_frames_per_packet(&mut self, frames: u32) {
        self.frames_per_packet = frames;
    }

    /// Get frames per packet
    #[must_use]
    pub fn frames_per_packet(&self) -> u32 {
        self.frames_per_packet
    }

    /// Set AES-128-CTR encryption keys (legacy)
    pub fn set_encryption(&mut self, key: [u8; 16], iv: [u8; 16]) {
        self.aes_key = Some(key);
        self.aes_iv = Some(iv);
        self.encryption_mode = RtpEncryptionMode::Aes128Ctr;
    }

    /// Set ChaCha20-Poly1305 encryption key (`AirPlay` 2)
    pub fn set_chacha_encryption(&mut self, key: [u8; 32]) {
        self.chacha_key = Some(key);
        self.encryption_mode = RtpEncryptionMode::ChaCha20Poly1305;
    }

    /// Get the encryption mode
    #[must_use]
    pub fn encryption_mode(&self) -> RtpEncryptionMode {
        self.encryption_mode
    }

    /// Enable buffered audio mode
    pub fn set_buffered_mode(&mut self, enabled: bool) {
        self.buffered_mode = enabled;
    }

    /// Reset sequence and timestamp
    pub fn reset(&mut self) {
        self.sequence = 0;
        self.timestamp = 0;
        self.nonce_counter = 0;
    }

    /// Get current sequence number
    #[must_use]
    pub fn sequence(&self) -> u16 {
        self.sequence
    }

    /// Get current timestamp
    #[must_use]
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Encode PCM audio to RTP packet
    ///
    /// Audio should be 16-bit signed little-endian stereo PCM.
    /// Expects exactly `frames_per_packet` * 4 bytes.
    ///
    /// # Errors
    ///
    /// Returns `RtpCodecError` if audio size is invalid or encryption fails.
    pub fn encode_audio(
        &mut self,
        pcm_data: &[u8],
        output: &mut Vec<u8>,
    ) -> Result<(), RtpCodecError> {
        let expected_size = self.frames_per_packet as usize * 4;
        if pcm_data.len() != expected_size {
            return Err(RtpCodecError::InvalidAudioSize(pcm_data.len()));
        }

        self.encode_arbitrary_payload(pcm_data, output)
    }

    /// Encode arbitrary audio payload (e.g. ALAC) to RTP packet
    ///
    /// # Errors
    ///
    /// Returns `RtpCodecError` if encryption fails.
    pub fn encode_arbitrary_payload(
        &mut self,
        data: &[u8],
        output: &mut Vec<u8>,
    ) -> Result<(), RtpCodecError> {
        // Create packet header
        let header =
            RtpHeader::new_audio(self.sequence, self.timestamp, self.ssrc, self.buffered_mode);
        let header_bytes = header.encode();

        match self.encryption_mode {
            RtpEncryptionMode::None => {
                // No encryption - just header + payload
                let packet = RtpPacket::new(header, data.to_vec());
                output.extend_from_slice(&packet.encode());
            }
            RtpEncryptionMode::Aes128Ctr => {
                // Legacy AES-128-CTR encryption
                let mut payload = data.to_vec();
                if let (Some(key), Some(iv)) = (&self.aes_key, &self.aes_iv) {
                    let mut cipher = Aes128Ctr::new(key, iv)
                        .map_err(|_| RtpCodecError::EncryptionNotInitialized)?;
                    // NOTE: AES-CTR seek might need adjustment for variable packet sizes if sequence is frame-based
                    // For now assuming frame-based seeking works or this is only used for fixed PCM in legacy mode.
                    let expected_size = self.frames_per_packet as usize * 4;
                    // Seek based on frame count, assuming 1:1 mapping if it was PCM.
                    // For ALAC, this logic might need review if legacy AirPlay 1 uses ALAC.
                    // But we are focusing on AirPlay 2 (ChaCha20).
                    cipher.seek(u64::from(self.sequence) * expected_size as u64);
                    cipher.apply_keystream(&mut payload);
                }
                let packet = RtpPacket::new(header, payload);
                output.extend_from_slice(&packet.encode());
            }
            RtpEncryptionMode::ChaCha20Poly1305 => {
                // ChaCha20-Poly1305 encryption (AirPlay 2)
                // Format: [Header (12)] [Encrypted Payload] [Tag (16)] [Nonce (8)]
                let key = self
                    .chacha_key
                    .as_ref()
                    .ok_or(RtpCodecError::EncryptionNotInitialized)?;

                let cipher = ChaCha20Poly1305Cipher::new(key)
                    .map_err(|e| RtpCodecError::EncryptionFailed(e.to_string()))?;

                // Generate 8-byte nonce (will be padded to 12 bytes internally)
                let nonce_bytes = self.nonce_counter.to_le_bytes();
                self.nonce_counter = self.nonce_counter.wrapping_add(1);

                // Create 12-byte nonce with 4-byte padding at start
                let mut full_nonce = [0u8; 12];
                full_nonce[4..12].copy_from_slice(&nonce_bytes);
                let nonce = Nonce::from_bytes(&full_nonce)
                    .map_err(|e| RtpCodecError::EncryptionFailed(e.to_string()))?;

                // AAD is timestamp (4 bytes) + SSRC (4 bytes) = bytes 4-12 of header
                let aad = &header_bytes[4..12];

                // Encrypt payload with AAD
                let encrypted = cipher
                    .encrypt_with_aad(&nonce, aad, data)
                    .map_err(|e| RtpCodecError::EncryptionFailed(e.to_string()))?;

                // encrypted contains: [ciphertext][tag (16 bytes)]
                // Split to get ciphertext and tag
                let (ciphertext, tag) = encrypted.split_at(encrypted.len() - Self::TAG_SIZE);

                // Build final packet: [header][ciphertext][tag][nonce (8 bytes)]
                output.reserve(
                    RtpHeader::SIZE + ciphertext.len() + Self::TAG_SIZE + Self::NONCE_SIZE,
                );
                output.extend_from_slice(&header_bytes);
                output.extend_from_slice(ciphertext);
                output.extend_from_slice(tag);
                output.extend_from_slice(&nonce_bytes);
            }
        }

        // Update state logic moved to caller or stays here?
        // encode_audio updated state. We should too.
        // But if ALAC has different frame count?
        // ALAC frames per packet is still 352 for AirPlay 2 (usually).

        // Update state for next packet
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(self.frames_per_packet);

        Ok(())
    }

    /// Encode multiple frames of audio
    ///
    /// Returns vector of encoded RTP packets
    ///
    /// # Errors
    ///
    /// Returns `RtpCodecError` if encryption fails.
    pub fn encode_audio_frames(&mut self, pcm_data: &[u8]) -> Result<Vec<Vec<u8>>, RtpCodecError> {
        let frame_size = self.frames_per_packet as usize * 4;
        let mut packets = Vec::new();

        for chunk in pcm_data.chunks(frame_size) {
            let mut packet = Vec::new();
            if chunk.len() == frame_size {
                self.encode_audio(chunk, &mut packet)?;
                packets.push(packet);
            } else if !chunk.is_empty() {
                // Pad last chunk with silence
                let mut padded = chunk.to_vec();
                padded.resize(frame_size, 0);
                self.encode_audio(&padded, &mut packet)?;
                packets.push(packet);
            }
        }

        Ok(packets)
    }

    /// Decode RTP packet
    ///
    /// # Errors
    ///
    /// Returns `RtpCodecError` if decryption fails or buffer is too small.
    pub fn decode_audio(&self, data: &[u8]) -> Result<RtpPacket, RtpCodecError> {
        match self.encryption_mode {
            RtpEncryptionMode::None => {
                // No decryption needed
                RtpPacket::decode(data).map_err(Into::into)
            }
            RtpEncryptionMode::Aes128Ctr => {
                // Legacy AES-128-CTR decryption
                let mut packet = RtpPacket::decode(data)?;
                if let (Some(key), Some(iv)) = (&self.aes_key, &self.aes_iv) {
                    let mut cipher = Aes128Ctr::new(key, iv)
                        .map_err(|_| RtpCodecError::EncryptionNotInitialized)?;
                    let frame_size = self.frames_per_packet as usize * 4;
                    cipher.seek(u64::from(packet.header.sequence) * frame_size as u64);
                    cipher.apply_keystream(&mut packet.payload);
                }
                Ok(packet)
            }
            RtpEncryptionMode::ChaCha20Poly1305 => {
                // ChaCha20-Poly1305 decryption
                // Format: [Header (12)] [Encrypted Payload] [Tag (16)] [Nonce (8)]
                let min_size = RtpHeader::SIZE + Self::TAG_SIZE + Self::NONCE_SIZE;
                if data.len() < min_size {
                    return Err(RtpCodecError::DecryptionFailed(
                        "packet too small for ChaCha20-Poly1305".to_string(),
                    ));
                }

                let header = RtpHeader::decode(data)?;

                // Extract nonce (last 8 bytes)
                let nonce_bytes = &data[data.len() - Self::NONCE_SIZE..];
                let mut full_nonce = [0u8; 12];
                full_nonce[4..12].copy_from_slice(nonce_bytes);
                let nonce = Nonce::from_bytes(&full_nonce)
                    .map_err(|e| RtpCodecError::DecryptionFailed(e.to_string()))?;

                // Extract tag (16 bytes before nonce)
                let tag_start = data.len() - Self::NONCE_SIZE - Self::TAG_SIZE;
                let tag = &data[tag_start..data.len() - Self::NONCE_SIZE];

                // Extract ciphertext (between header and tag)
                let ciphertext = &data[RtpHeader::SIZE..tag_start];

                // AAD is timestamp + SSRC (bytes 4-12 of header)
                let aad = &data[4..12];

                // Combine ciphertext + tag for decryption
                let mut encrypted = ciphertext.to_vec();
                encrypted.extend_from_slice(tag);

                let key = self
                    .chacha_key
                    .as_ref()
                    .ok_or(RtpCodecError::EncryptionNotInitialized)?;

                let cipher = ChaCha20Poly1305Cipher::new(key)
                    .map_err(|e| RtpCodecError::DecryptionFailed(e.to_string()))?;

                let payload = cipher
                    .decrypt_with_aad(&nonce, aad, &encrypted)
                    .map_err(|e| RtpCodecError::DecryptionFailed(e.to_string()))?;

                Ok(RtpPacket::new(header, payload))
            }
        }
    }
}

/// Builder for audio packet batches
pub struct AudioPacketBuilder {
    codec: RtpCodec,
    packets: Vec<Vec<u8>>,
}

impl AudioPacketBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new(ssrc: u32) -> Self {
        Self {
            codec: RtpCodec::new(ssrc),
            packets: Vec::new(),
        }
    }

    /// Set AES-128-CTR encryption (legacy)
    #[must_use]
    pub fn with_encryption(mut self, key: [u8; 16], iv: [u8; 16]) -> Self {
        self.codec.set_encryption(key, iv);
        self
    }

    /// Set ChaCha20-Poly1305 encryption (`AirPlay` 2)
    #[must_use]
    pub fn with_chacha_encryption(mut self, key: [u8; 32]) -> Self {
        self.codec.set_chacha_encryption(key);
        self
    }

    /// Add audio data
    ///
    /// # Errors
    ///
    /// Returns `RtpCodecError` if audio processing fails.
    pub fn add_audio(mut self, pcm_data: &[u8]) -> Result<Self, RtpCodecError> {
        let new_packets = self.codec.encode_audio_frames(pcm_data)?;
        self.packets.extend(new_packets);
        Ok(self)
    }

    /// Build all packets
    #[must_use]
    pub fn build(self) -> Vec<Vec<u8>> {
        self.packets
    }
}
