use super::packet::{RtpDecodeError, RtpHeader, RtpPacket};
use crate::protocol::crypto::Aes128Ctr;
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
    /// Use buffered audio mode
    buffered_mode: bool,
}

impl RtpCodec {
    /// Samples per packet
    pub const FRAMES_PER_PACKET: u32 = 352;

    /// Create a new codec
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            sequence: 0,
            timestamp: 0,
            aes_key: None,
            aes_iv: None,
            buffered_mode: false,
        }
    }

    /// Set encryption keys
    pub fn set_encryption(&mut self, key: [u8; 16], iv: [u8; 16]) {
        self.aes_key = Some(key);
        self.aes_iv = Some(iv);
    }

    /// Enable buffered audio mode
    pub fn set_buffered_mode(&mut self, enabled: bool) {
        self.buffered_mode = enabled;
    }

    /// Reset sequence and timestamp
    pub fn reset(&mut self) {
        self.sequence = 0;
        self.timestamp = 0;
    }

    /// Get current sequence number
    pub fn sequence(&self) -> u16 {
        self.sequence
    }

    /// Get current timestamp
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Encode PCM audio to RTP packet
    ///
    /// Audio should be 16-bit signed little-endian stereo PCM.
    /// Expects exactly FRAMES_PER_PACKET * 4 bytes (352 frames * 4 bytes/frame).
    pub fn encode_audio(&mut self, pcm_data: &[u8]) -> Result<Vec<u8>, RtpCodecError> {
        let expected_size = Self::FRAMES_PER_PACKET as usize * 4;
        if pcm_data.len() != expected_size {
            return Err(RtpCodecError::InvalidAudioSize(pcm_data.len()));
        }

        // Create packet
        let header =
            RtpHeader::new_audio(self.sequence, self.timestamp, self.ssrc, self.buffered_mode);

        let mut payload = pcm_data.to_vec();

        // Encrypt if keys are set
        if let (Some(key), Some(iv)) = (&self.aes_key, &self.aes_iv) {
            let mut cipher =
                Aes128Ctr::new(key, iv).map_err(|_| RtpCodecError::EncryptionNotInitialized)?;

            // Seek to correct position based on packet index
            // AirPlay uses sequence number for CTR position
            cipher.seek((self.sequence as u64) * expected_size as u64);
            cipher.apply_keystream(&mut payload);
        }

        // Update state for next packet
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(Self::FRAMES_PER_PACKET);

        // Build final packet
        let packet = RtpPacket::new(header, payload);
        Ok(packet.encode())
    }

    /// Encode multiple frames of audio
    ///
    /// Returns vector of encoded RTP packets
    pub fn encode_audio_frames(&mut self, pcm_data: &[u8]) -> Result<Vec<Vec<u8>>, RtpCodecError> {
        let frame_size = Self::FRAMES_PER_PACKET as usize * 4;
        let mut packets = Vec::new();

        for chunk in pcm_data.chunks(frame_size) {
            if chunk.len() == frame_size {
                packets.push(self.encode_audio(chunk)?);
            } else if !chunk.is_empty() {
                // Pad last chunk with silence
                let mut padded = chunk.to_vec();
                padded.resize(frame_size, 0);
                packets.push(self.encode_audio(&padded)?);
            }
        }

        Ok(packets)
    }

    /// Decode RTP packet
    pub fn decode_audio(&self, data: &[u8]) -> Result<RtpPacket, RtpCodecError> {
        let mut packet = RtpPacket::decode(data)?;

        // Decrypt if keys are set
        if let (Some(key), Some(iv)) = (&self.aes_key, &self.aes_iv) {
            let mut cipher =
                Aes128Ctr::new(key, iv).map_err(|_| RtpCodecError::EncryptionNotInitialized)?;

            let frame_size = Self::FRAMES_PER_PACKET as usize * 4;
            cipher.seek((packet.header.sequence as u64) * frame_size as u64);
            cipher.apply_keystream(&mut packet.payload);
        }

        Ok(packet)
    }
}

/// Builder for audio packet batches
pub struct AudioPacketBuilder {
    codec: RtpCodec,
    packets: Vec<Vec<u8>>,
}

impl AudioPacketBuilder {
    /// Create a new builder
    pub fn new(ssrc: u32) -> Self {
        Self {
            codec: RtpCodec::new(ssrc),
            packets: Vec::new(),
        }
    }

    /// Set encryption
    pub fn with_encryption(mut self, key: [u8; 16], iv: [u8; 16]) -> Self {
        self.codec.set_encryption(key, iv);
        self
    }

    /// Add audio data
    pub fn add_audio(mut self, pcm_data: &[u8]) -> Result<Self, RtpCodecError> {
        let new_packets = self.codec.encode_audio_frames(pcm_data)?;
        self.packets.extend(new_packets);
        Ok(self)
    }

    /// Build all packets
    pub fn build(self) -> Vec<Vec<u8>> {
        self.packets
    }
}
