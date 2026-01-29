# Section 54: RTP Audio Receiver (AirPlay 2)

## Dependencies
- **Section 52**: Multi-Phase SETUP Handler (port allocation)
- **Section 05**: RTP Protocol (packet structures)
- **Section 04**: Cryptographic Primitives (ChaCha20-Poly1305)
- **Section 39**: RTP Receiver Core (AirPlay 1 patterns)

## Overview

This section implements the RTP audio receiver for AirPlay 2. Unlike AirPlay 1 which uses AES-128-CTR, AirPlay 2 encrypts audio packets with ChaCha20-Poly1305 AEAD.

### RTP Packet Flow

```
Sender ──── RTP/UDP (encrypted) ────▶ Receiver
                                       │
                                       ▼
                              ┌────────────────┐
                              │ Decrypt        │
                              │ ChaCha20-Poly1305│
                              └────────┬───────┘
                                       │
                                       ▼
                              ┌────────────────┐
                              │ Decode Audio   │
                              │ (ALAC/AAC/PCM) │
                              └────────┬───────┘
                                       │
                                       ▼
                              ┌────────────────┐
                              │ Jitter Buffer  │
                              │ (Section 56)   │
                              └────────────────┘
```

## Objectives

- Receive RTP packets on allocated UDP port
- Decrypt audio payloads using ChaCha20-Poly1305
- Parse RTP headers and extract audio data
- Handle packet loss and reordering
- Support ALAC, AAC-ELD, and PCM formats
- Reuse existing RTP packet structures

---

## Tasks

### 54.1 RTP Decryptor

- [ ] **54.1.1** Implement RTP payload decryption

**File:** `src/receiver/ap2/rtp_decryptor.rs`

```rust
//! RTP Audio Decryption for AirPlay 2
//!
//! Decrypts RTP audio payloads using ChaCha20-Poly1305 AEAD.

use crate::protocol::crypto::chacha::ChaCha20Poly1305;
use crate::protocol::rtp::RtpPacket;

/// AirPlay 2 RTP decryptor
pub struct Ap2RtpDecryptor {
    /// Decryption key (from SETUP)
    key: [u8; 32],
    /// AAD (Additional Authenticated Data) prefix
    aad_prefix: Option<Vec<u8>>,
}

impl Ap2RtpDecryptor {
    /// Create a new decryptor with the shared key from SETUP
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
    pub fn decrypt(&self, packet: &RtpPacket) -> Result<Vec<u8>, DecryptionError> {
        let payload = &packet.payload;

        if payload.len() < 16 {
            return Err(DecryptionError::PayloadTooShort);
        }

        // Build nonce from RTP header
        let nonce = self.build_nonce(packet);

        // Build AAD if configured
        let aad = self.build_aad(packet);

        // Decrypt with AEAD
        let cipher = ChaCha20Poly1305::new(&self.key);

        let plaintext = if let Some(ref aad) = aad {
            cipher.decrypt_with_aad(&nonce, payload, aad)
        } else {
            cipher.decrypt(&nonce, payload)
        }.map_err(|_| DecryptionError::AuthenticationFailed)?;

        Ok(plaintext)
    }

    /// Build nonce from RTP header fields
    ///
    /// Nonce format for AirPlay 2:
    /// - 4 bytes: zeros
    /// - 4 bytes: SSRC (big-endian)
    /// - 4 bytes: sequence + timestamp bits
    fn build_nonce(&self, packet: &RtpPacket) -> [u8; 12] {
        let mut nonce = [0u8; 12];

        // SSRC at offset 4
        nonce[4..8].copy_from_slice(&packet.ssrc.to_be_bytes());

        // Sequence at offset 8 (extended to 4 bytes)
        nonce[8..10].copy_from_slice(&packet.sequence.to_be_bytes());

        // Could include timestamp bits in remaining bytes
        // This varies by implementation

        nonce
    }

    /// Build AAD from RTP header
    fn build_aad(&self, packet: &RtpPacket) -> Option<Vec<u8>> {
        self.aad_prefix.as_ref().map(|prefix| {
            let mut aad = prefix.clone();
            // Add RTP header bytes
            aad.extend_from_slice(&[
                (packet.version << 6) | (packet.padding as u8) << 5 |
                    (packet.extension as u8) << 4 | packet.csrc_count,
                (packet.marker as u8) << 7 | packet.payload_type,
            ]);
            aad.extend_from_slice(&packet.sequence.to_be_bytes());
            aad.extend_from_slice(&packet.timestamp.to_be_bytes());
            aad.extend_from_slice(&packet.ssrc.to_be_bytes());
            aad
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptionError {
    #[error("Payload too short (< 16 bytes for auth tag)")]
    PayloadTooShort,

    #[error("Authentication failed - corrupted or tampered packet")]
    AuthenticationFailed,
}

/// Audio format handler
pub trait AudioDecoder: Send + Sync {
    /// Decode audio data to PCM samples
    fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>, AudioDecodeError>;

    /// Get sample rate
    fn sample_rate(&self) -> u32;

    /// Get channel count
    fn channels(&self) -> u8;
}

#[derive(Debug, thiserror::Error)]
pub enum AudioDecodeError {
    #[error("Invalid audio data")]
    InvalidData,

    #[error("Unsupported format")]
    UnsupportedFormat,

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
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8) -> Self {
        Self { sample_rate, channels, bits_per_sample }
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
                let samples: Vec<i16> = data
                    .chunks_exact(3)
                    .map(|chunk| {
                        let value = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]);
                        (value >> 8) as i16
                    })
                    .collect();
                Ok(samples)
            }
            _ => Err(AudioDecodeError::UnsupportedFormat),
        }
    }

    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn channels(&self) -> u8 { self.channels }
}

/// ALAC decoder wrapper
pub struct AlacDecoder {
    // Would wrap alac-decoder crate
    sample_rate: u32,
    channels: u8,
}

impl AlacDecoder {
    pub fn new(sample_rate: u32, channels: u8, _magic_cookie: &[u8]) -> Result<Self, AudioDecodeError> {
        // Initialize ALAC decoder with magic cookie
        Ok(Self { sample_rate, channels })
    }
}

impl AudioDecoder for AlacDecoder {
    fn decode(&mut self, _data: &[u8]) -> Result<Vec<i16>, AudioDecodeError> {
        // Would call into alac-decoder crate
        // For now, placeholder
        Err(AudioDecodeError::DecoderError("ALAC decoder not implemented".into()))
    }

    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn channels(&self) -> u8 { self.channels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_building() {
        let decryptor = Ap2RtpDecryptor::new([0u8; 32]);

        let packet = RtpPacket {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: false,
            payload_type: 96,
            sequence: 0x1234,
            timestamp: 0x56789ABC,
            ssrc: 0xDEADBEEF,
            csrc: vec![],
            payload: vec![],
        };

        let nonce = decryptor.build_nonce(&packet);

        // First 4 bytes are zero
        assert_eq!(&nonce[0..4], &[0, 0, 0, 0]);
        // SSRC at offset 4 (big-endian)
        assert_eq!(&nonce[4..8], &[0xDE, 0xAD, 0xBE, 0xEF]);
        // Sequence at offset 8
        assert_eq!(&nonce[8..10], &[0x12, 0x34]);
    }

    #[test]
    fn test_pcm_decoder_16bit() {
        let mut decoder = PcmDecoder::new(44100, 2, 16);

        // Two 16-bit samples
        let data = [0x00, 0x40, 0x00, 0xC0];  // 16384, -16384
        let samples = decoder.decode(&data).unwrap();

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0], 16384);
        assert_eq!(samples[1], -16384);
    }
}
```

---

### 54.2 RTP Receiver

- [ ] **54.2.1** Implement UDP receiver for RTP packets

**File:** `src/receiver/ap2/rtp_receiver.rs`

```rust
//! RTP Packet Receiver
//!
//! Receives RTP packets on the allocated UDP port and processes them.

use super::rtp_decryptor::{Ap2RtpDecryptor, AudioDecoder, DecryptionError};
use crate::protocol::rtp::{RtpPacket, RtpCodec};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::net::UdpSocket;

/// Received audio frame
#[derive(Debug)]
pub struct AudioFrame {
    /// RTP sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// Decoded PCM samples (interleaved stereo)
    pub samples: Vec<i16>,
    /// Receive time (for jitter calculation)
    pub receive_time: std::time::Instant,
}

/// RTP receiver configuration
#[derive(Debug, Clone)]
pub struct RtpReceiverConfig {
    /// UDP port to listen on
    pub port: u16,
    /// Decryption key
    pub key: [u8; 32],
    /// Audio sample rate
    pub sample_rate: u32,
    /// Audio channels
    pub channels: u8,
    /// Bits per sample
    pub bits_per_sample: u8,
    /// Codec type (96=ALAC, 100=PCM, etc.)
    pub codec_type: u8,
}

/// RTP receiver
pub struct RtpReceiver {
    config: RtpReceiverConfig,
    decryptor: Ap2RtpDecryptor,
    decoder: Box<dyn AudioDecoder>,
    rtp_codec: RtpCodec,
    /// Channel to send decoded frames
    frame_tx: mpsc::Sender<AudioFrame>,
    /// Statistics
    stats: ReceiverStats,
}

#[derive(Debug, Default)]
pub struct ReceiverStats {
    pub packets_received: u64,
    pub packets_decrypted: u64,
    pub packets_failed: u64,
    pub bytes_received: u64,
    pub samples_decoded: u64,
    pub last_sequence: u16,
    pub sequence_gaps: u64,
}

impl RtpReceiver {
    pub fn new(
        config: RtpReceiverConfig,
        frame_tx: mpsc::Sender<AudioFrame>,
    ) -> Result<Self, ReceiverError> {
        let decryptor = Ap2RtpDecryptor::new(config.key);

        // Create appropriate decoder based on codec type
        let decoder: Box<dyn AudioDecoder> = match config.codec_type {
            100 => Box::new(super::rtp_decryptor::PcmDecoder::new(
                config.sample_rate,
                config.channels,
                config.bits_per_sample,
            )),
            96 => {
                // ALAC - would need magic cookie from SETUP
                return Err(ReceiverError::UnsupportedCodec(config.codec_type));
            }
            _ => return Err(ReceiverError::UnsupportedCodec(config.codec_type)),
        };

        Ok(Self {
            config,
            decryptor,
            decoder,
            rtp_codec: RtpCodec::new(),
            frame_tx,
            stats: ReceiverStats::default(),
        })
    }

    /// Process a received UDP packet
    pub fn process_packet(&mut self, data: &[u8]) -> Result<(), ReceiverError> {
        self.stats.packets_received += 1;
        self.stats.bytes_received += data.len() as u64;

        // Parse RTP header
        let packet = RtpPacket::parse(data)
            .map_err(|e| ReceiverError::ParseError(e.to_string()))?;

        // Check for sequence gaps
        let expected_seq = self.stats.last_sequence.wrapping_add(1);
        if self.stats.packets_received > 1 && packet.sequence != expected_seq {
            self.stats.sequence_gaps += 1;
            log::warn!(
                "Sequence gap: expected {}, got {}",
                expected_seq,
                packet.sequence
            );
        }
        self.stats.last_sequence = packet.sequence;

        // Decrypt payload
        let decrypted = self.decryptor.decrypt(&packet)
            .map_err(|e| {
                self.stats.packets_failed += 1;
                ReceiverError::DecryptError(e)
            })?;

        self.stats.packets_decrypted += 1;

        // Decode audio
        let samples = self.decoder.decode(&decrypted)
            .map_err(|e| ReceiverError::DecodeError(e.to_string()))?;

        self.stats.samples_decoded += samples.len() as u64;

        // Create frame
        let frame = AudioFrame {
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            samples,
            receive_time: std::time::Instant::now(),
        };

        // Send to jitter buffer
        self.frame_tx.try_send(frame)
            .map_err(|_| ReceiverError::ChannelFull)?;

        Ok(())
    }

    /// Get receiver statistics
    pub fn stats(&self) -> &ReceiverStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ReceiverStats::default();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("Failed to parse RTP packet: {0}")]
    ParseError(String),

    #[error("Decryption failed: {0}")]
    DecryptError(DecryptionError),

    #[error("Audio decode error: {0}")]
    DecodeError(String),

    #[error("Unsupported codec type: {0}")]
    UnsupportedCodec(u8),

    #[error("Frame channel full")]
    ChannelFull,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Async receiver task
pub async fn run_receiver(
    socket: UdpSocket,
    mut receiver: RtpReceiver,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), ReceiverError> {
    let mut buf = [0u8; 2048];

    loop {
        tokio::select! {
            result = socket.recv(&mut buf) => {
                match result {
                    Ok(len) => {
                        if let Err(e) = receiver.process_packet(&buf[..len]) {
                            log::warn!("Packet processing error: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Socket receive error: {}", e);
                    }
                }
            }
            _ = shutdown.recv() => {
                log::info!("RTP receiver shutting down");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_receiver_stats() {
        let (tx, _rx) = mpsc::channel(10);
        let config = RtpReceiverConfig {
            port: 7100,
            key: [0u8; 32],
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            codec_type: 100,  // PCM
        };

        let receiver = RtpReceiver::new(config, tx).unwrap();
        assert_eq!(receiver.stats().packets_received, 0);
    }
}
```

---

## Acceptance Criteria

- [ ] RTP packets received on configured UDP port
- [ ] ChaCha20-Poly1305 decryption of audio payloads
- [ ] Correct nonce construction from RTP header
- [ ] PCM decoding (16-bit, 24-bit)
- [ ] Sequence number tracking and gap detection
- [ ] Statistics collection
- [ ] Frames passed to jitter buffer
- [ ] All unit tests pass

---

## Notes

### Encryption Key Source

The decryption key comes from the SETUP request's `shk` (shared key) field, which was encrypted with the pairing session key.

### Codec Support

- **PCM**: Direct passthrough (Section 54)
- **ALAC**: Requires magic cookie from SETUP, use `alac-encoder` crate (decode mode)
- **AAC-ELD**: Requires external decoder (fdk-aac or similar)

---

## References

- [RFC 3550: RTP](https://tools.ietf.org/html/rfc3550)
- [Section 05: RTP Protocol](./complete/05-rtsp-protocol.md)
- [Section 39: RTP Receiver Core](./complete/39-rtp-receiver-core.md)
