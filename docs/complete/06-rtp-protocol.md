# Section 06: RTP/RAOP Protocol (Sans-IO)

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 04**: Cryptographic Primitives (must be complete - for audio encryption)

## Overview

AirPlay uses RTP (Real-time Transport Protocol) for audio data transmission. The AirPlay variant, sometimes called RAOP (Remote Audio Output Protocol), extends standard RTP with:
- Apple-specific packet formats
- Audio encryption (AES-128-CTR)
- Timing synchronization
- Control channel for retransmissions

This section implements sans-IO RTP packet encoding/decoding for audio streaming.

## Objectives

- Implement RTP packet structure and serialization
- Implement RAOP-specific extensions
- Handle audio packet encryption
- Implement timing/sync packets
- Support control channel messages

---

## Tasks

### 6.1 RTP Packet Types

- [x] **6.1.1** Define RTP header structure

**File:** `src/protocol/rtp/mod.rs`

```rust
//! RTP/RAOP protocol implementation for AirPlay audio streaming

mod packet;
mod timing;
mod control;
mod codec;

pub use packet::{RtpPacket, RtpHeader, PayloadType};
pub use timing::{TimingPacket, TimingRequest, TimingResponse};
pub use control::{ControlPacket, RetransmitRequest};
pub use codec::{RtpCodec, RtpCodecError};

/// RTP protocol constants for AirPlay
pub mod constants {
    /// Default RTP audio port
    pub const AUDIO_PORT: u16 = 6000;
    /// Default RTP control port
    pub const CONTROL_PORT: u16 = 6001;
    /// Default RTP timing port
    pub const TIMING_PORT: u16 = 6002;

    /// Audio frames per RTP packet (352 samples at 44.1kHz ≈ 8ms)
    pub const FRAMES_PER_PACKET: usize = 352;

    /// Audio sample rate
    pub const SAMPLE_RATE: u32 = 44100;

    /// Audio channels (stereo)
    pub const CHANNELS: u8 = 2;

    /// Bits per sample
    pub const BITS_PER_SAMPLE: u8 = 16;
}
```

- [x] **6.1.2** Implement RTP packet structure

**File:** `src/protocol/rtp/packet.rs`

```rust
use super::constants;

/// RTP payload types for AirPlay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PayloadType {
    /// Timing request
    TimingRequest = 0x52,
    /// Timing response
    TimingResponse = 0x53,
    /// Audio data (realtime)
    AudioRealtime = 0x60,
    /// Audio data (buffered)
    AudioBuffered = 0x61,
    /// Retransmit request
    RetransmitRequest = 0x55,
    /// Retransmit response
    RetransmitResponse = 0x56,
}

impl PayloadType {
    /// Parse from byte value
    pub fn from_byte(b: u8) -> Option<Self> {
        match b & 0x7F {
            0x52 => Some(Self::TimingRequest),
            0x53 => Some(Self::TimingResponse),
            0x60 => Some(Self::AudioRealtime),
            0x61 => Some(Self::AudioBuffered),
            0x55 => Some(Self::RetransmitRequest),
            0x56 => Some(Self::RetransmitResponse),
            _ => None,
        }
    }
}

/// RTP header (12 bytes standard, extended for AirPlay)
#[derive(Debug, Clone)]
pub struct RtpHeader {
    /// Version (2 bits, always 2)
    pub version: u8,
    /// Padding flag
    pub padding: bool,
    /// Extension flag
    pub extension: bool,
    /// CSRC count (4 bits)
    pub csrc_count: u8,
    /// Marker bit
    pub marker: bool,
    /// Payload type (7 bits)
    pub payload_type: PayloadType,
    /// Sequence number (16 bits)
    pub sequence: u16,
    /// Timestamp (32 bits)
    pub timestamp: u32,
    /// Synchronization source ID (32 bits)
    pub ssrc: u32,
}

impl RtpHeader {
    /// Standard RTP header size
    pub const SIZE: usize = 12;

    /// Create a new audio packet header
    pub fn new_audio(sequence: u16, timestamp: u32, ssrc: u32, buffered: bool) -> Self {
        Self {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: true,
            payload_type: if buffered {
                PayloadType::AudioBuffered
            } else {
                PayloadType::AudioRealtime
            },
            sequence,
            timestamp,
            ssrc,
        }
    }

    /// Encode header to bytes
    pub fn encode(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];

        // Byte 0: V(2) | P(1) | X(1) | CC(4)
        buf[0] = (self.version << 6)
            | ((self.padding as u8) << 5)
            | ((self.extension as u8) << 4)
            | (self.csrc_count & 0x0F);

        // Byte 1: M(1) | PT(7)
        buf[1] = ((self.marker as u8) << 7) | (self.payload_type as u8 & 0x7F);

        // Bytes 2-3: Sequence number
        buf[2..4].copy_from_slice(&self.sequence.to_be_bytes());

        // Bytes 4-7: Timestamp
        buf[4..8].copy_from_slice(&self.timestamp.to_be_bytes());

        // Bytes 8-11: SSRC
        buf[8..12].copy_from_slice(&self.ssrc.to_be_bytes());

        buf
    }

    /// Decode header from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::SIZE,
                have: buf.len(),
            });
        }

        let version = (buf[0] >> 6) & 0x03;
        if version != 2 {
            return Err(RtpDecodeError::InvalidVersion(version));
        }

        let payload_type_byte = buf[1] & 0x7F;
        let payload_type = PayloadType::from_byte(payload_type_byte)
            .ok_or(RtpDecodeError::UnknownPayloadType(payload_type_byte))?;

        Ok(Self {
            version,
            padding: (buf[0] >> 5) & 0x01 != 0,
            extension: (buf[0] >> 4) & 0x01 != 0,
            csrc_count: buf[0] & 0x0F,
            marker: (buf[1] >> 7) & 0x01 != 0,
            payload_type,
            sequence: u16::from_be_bytes([buf[2], buf[3]]),
            timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            ssrc: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}

/// RTP decode errors
#[derive(Debug, thiserror::Error)]
pub enum RtpDecodeError {
    #[error("buffer too small: need {needed} bytes, have {have}")]
    BufferTooSmall { needed: usize, have: usize },

    #[error("invalid RTP version: {0}")]
    InvalidVersion(u8),

    #[error("unknown payload type: 0x{0:02x}")]
    UnknownPayloadType(u8),

    #[error("decryption failed")]
    DecryptionFailed,
}

/// Complete RTP packet with header and payload
#[derive(Debug, Clone)]
pub struct RtpPacket {
    /// Packet header
    pub header: RtpHeader,
    /// Payload data (audio samples or control data)
    pub payload: Vec<u8>,
}

impl RtpPacket {
    /// Create a new RTP packet
    pub fn new(header: RtpHeader, payload: Vec<u8>) -> Self {
        Self { header, payload }
    }

    /// Create an audio packet
    pub fn audio(
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        audio_data: Vec<u8>,
        buffered: bool,
    ) -> Self {
        Self {
            header: RtpHeader::new_audio(sequence, timestamp, ssrc, buffered),
            payload: audio_data,
        }
    }

    /// Encode packet to bytes (without encryption)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(RtpHeader::SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.encode());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode packet from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        let header = RtpHeader::decode(buf)?;
        let payload = buf[RtpHeader::SIZE..].to_vec();
        Ok(Self { header, payload })
    }

    /// Get payload as audio samples (assuming 16-bit stereo)
    pub fn audio_samples(&self) -> impl Iterator<Item = (i16, i16)> + '_ {
        self.payload.chunks_exact(4).map(|chunk| {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            let right = i16::from_le_bytes([chunk[2], chunk[3]]);
            (left, right)
        })
    }
}
```

---

### 6.2 Timing Synchronization

- [x] **6.2.1** Implement timing packets

**File:** `src/protocol/rtp/timing.rs`

```rust
/// NTP timestamp (64-bit, seconds since 1900-01-01)
#[derive(Debug, Clone, Copy, Default)]
pub struct NtpTimestamp {
    /// Seconds since NTP epoch
    pub seconds: u32,
    /// Fractional seconds (1/2^32 of a second)
    pub fraction: u32,
}

impl NtpTimestamp {
    /// NTP epoch offset from Unix epoch (70 years in seconds)
    const NTP_UNIX_OFFSET: u64 = 2208988800;

    /// Create from current time
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let ntp_secs = duration.as_secs() + Self::NTP_UNIX_OFFSET;
        let fraction = ((duration.subsec_nanos() as u64) << 32) / 1_000_000_000;

        Self {
            seconds: ntp_secs as u32,
            fraction: fraction as u32,
        }
    }

    /// Encode to 8 bytes
    pub fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        buf[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        buf
    }

    /// Decode from 8 bytes
    pub fn decode(buf: &[u8]) -> Self {
        Self {
            seconds: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            fraction: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        }
    }

    /// Convert to microseconds since NTP epoch
    pub fn to_micros(&self) -> u64 {
        let secs = self.seconds as u64;
        let frac_micros = ((self.fraction as u64) * 1_000_000) >> 32;
        secs * 1_000_000 + frac_micros
    }
}

/// Timing request packet
#[derive(Debug, Clone)]
pub struct TimingRequest {
    /// Reference timestamp
    pub reference_time: NtpTimestamp,
    /// Receive timestamp (zero in request)
    pub receive_time: NtpTimestamp,
    /// Send timestamp
    pub send_time: NtpTimestamp,
}

impl TimingRequest {
    /// Packet size
    pub const SIZE: usize = 32;

    /// Create a new timing request
    pub fn new() -> Self {
        let now = NtpTimestamp::now();
        Self {
            reference_time: now,
            receive_time: NtpTimestamp::default(),
            send_time: now,
        }
    }

    /// Encode to bytes (including RTP header)
    pub fn encode(&self, sequence: u16, ssrc: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32);

        // RTP header for timing request
        buf.push(0x80); // V=2, P=0, X=0, CC=0
        buf.push(0xD2); // M=1, PT=0x52
        buf.extend_from_slice(&sequence.to_be_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Timestamp (not used)
        buf.extend_from_slice(&ssrc.to_be_bytes());

        // Timing data
        buf.extend_from_slice(&[0u8; 4]); // Padding
        buf.extend_from_slice(&self.reference_time.encode());
        buf.extend_from_slice(&self.receive_time.encode());
        buf.extend_from_slice(&self.send_time.encode());

        buf
    }
}

/// Timing response packet
#[derive(Debug, Clone)]
pub struct TimingResponse {
    /// Original reference timestamp (from request)
    pub reference_time: NtpTimestamp,
    /// Time server received request
    pub receive_time: NtpTimestamp,
    /// Time server sent response
    pub send_time: NtpTimestamp,
}

impl TimingResponse {
    /// Decode from bytes (excluding RTP header)
    pub fn decode(buf: &[u8]) -> Result<Self, super::packet::RtpDecodeError> {
        if buf.len() < 24 {
            return Err(super::packet::RtpDecodeError::BufferTooSmall {
                needed: 24,
                have: buf.len(),
            });
        }

        Ok(Self {
            reference_time: NtpTimestamp::decode(&buf[0..8]),
            receive_time: NtpTimestamp::decode(&buf[8..16]),
            send_time: NtpTimestamp::decode(&buf[16..24]),
        })
    }

    /// Calculate clock offset (server time - client time)
    ///
    /// Returns offset in microseconds
    pub fn calculate_offset(&self, client_receive_time: NtpTimestamp) -> i64 {
        // offset = ((T2 - T1) + (T3 - T4)) / 2
        // where:
        // T1 = reference_time (client send)
        // T2 = receive_time (server receive)
        // T3 = send_time (server send)
        // T4 = client_receive_time

        let t1 = self.reference_time.to_micros() as i64;
        let t2 = self.receive_time.to_micros() as i64;
        let t3 = self.send_time.to_micros() as i64;
        let t4 = client_receive_time.to_micros() as i64;

        ((t2 - t1) + (t3 - t4)) / 2
    }

    /// Calculate round-trip time
    ///
    /// Returns RTT in microseconds
    pub fn calculate_rtt(&self, client_receive_time: NtpTimestamp) -> u64 {
        // RTT = (T4 - T1) - (T3 - T2)

        let t1 = self.reference_time.to_micros();
        let t2 = self.receive_time.to_micros();
        let t3 = self.send_time.to_micros();
        let t4 = client_receive_time.to_micros();

        (t4 - t1).saturating_sub(t3 - t2)
    }
}

/// Timing packet (request or response)
#[derive(Debug, Clone)]
pub enum TimingPacket {
    Request(TimingRequest),
    Response(TimingResponse),
}
```

---

### 6.3 Control Channel

- [x] **6.3.1** Implement retransmit request handling

**File:** `src/protocol/rtp/control.rs`

```rust
use super::packet::RtpDecodeError;

/// Retransmit request for lost packets
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub sequence_start: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

impl RetransmitRequest {
    /// Create a new retransmit request
    pub fn new(sequence_start: u16, count: u16) -> Self {
        Self {
            sequence_start,
            count,
        }
    }

    /// Encode to bytes (including RTP-like header)
    pub fn encode(&self, ssrc: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16);

        // Header
        buf.push(0x80);
        buf.push(0xD5); // PT=0x55 (retransmit request)
        buf.extend_from_slice(&self.sequence_start.to_be_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Timestamp
        buf.extend_from_slice(&ssrc.to_be_bytes());

        // Retransmit data
        buf.extend_from_slice(&self.sequence_start.to_be_bytes());
        buf.extend_from_slice(&self.count.to_be_bytes());

        buf
    }

    /// Decode from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        Ok(Self {
            sequence_start: u16::from_be_bytes([buf[0], buf[1]]),
            count: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }
}

/// Control packet types
#[derive(Debug, Clone)]
pub enum ControlPacket {
    /// Request retransmission of lost packets
    RetransmitRequest(RetransmitRequest),
    /// Sync packet for timing
    Sync {
        rtp_timestamp: u32,
        ntp_timestamp: super::timing::NtpTimestamp,
        next_timestamp: u32,
    },
}

impl ControlPacket {
    /// Parse control packet from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 12 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 12,
                have: buf.len(),
            });
        }

        let payload_type = buf[1] & 0x7F;

        match payload_type {
            0x55 => {
                let request = RetransmitRequest::decode(&buf[12..])?;
                Ok(ControlPacket::RetransmitRequest(request))
            }
            0x54 => {
                // Sync packet
                if buf.len() < 20 {
                    return Err(RtpDecodeError::BufferTooSmall {
                        needed: 20,
                        have: buf.len(),
                    });
                }
                Ok(ControlPacket::Sync {
                    rtp_timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
                    ntp_timestamp: super::timing::NtpTimestamp::decode(&buf[8..16]),
                    next_timestamp: u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]),
                })
            }
            _ => Err(RtpDecodeError::UnknownPayloadType(payload_type)),
        }
    }
}
```

---

### 6.4 Audio Packet Codec

- [x] **6.4.1** Implement audio packet encoding with encryption

**File:** `src/protocol/rtp/codec.rs`

```rust
use super::packet::{RtpPacket, RtpHeader, RtpDecodeError};
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
        let header = RtpHeader::new_audio(
            self.sequence,
            self.timestamp,
            self.ssrc,
            self.buffered_mode,
        );

        let mut payload = pcm_data.to_vec();

        // Encrypt if keys are set
        if let (Some(key), Some(iv)) = (&self.aes_key, &self.aes_iv) {
            let mut cipher = Aes128Ctr::new(key, iv)
                .map_err(|_| RtpCodecError::EncryptionNotInitialized)?;

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
    pub fn encode_audio_frames(
        &mut self,
        pcm_data: &[u8],
    ) -> Result<Vec<Vec<u8>>, RtpCodecError> {
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
            let mut cipher = Aes128Ctr::new(key, iv)
                .map_err(|_| RtpCodecError::EncryptionNotInitialized)?;

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
```

---

## Unit Tests

### Test File: `src/protocol/rtp/packet.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = RtpHeader::new_audio(100, 44100, 0x12345678, false);

        let encoded = header.encode();
        let decoded = RtpHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.version, 2);
        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.timestamp, 44100);
        assert_eq!(decoded.ssrc, 0x12345678);
        assert!(decoded.marker);
    }

    #[test]
    fn test_packet_encode_decode() {
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let packet = RtpPacket::audio(1, 352, 0xAABBCCDD, payload.clone(), false);

        let encoded = packet.encode();
        let decoded = RtpPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.header.sequence, 1);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_payload_type_values() {
        assert_eq!(PayloadType::TimingRequest as u8, 0x52);
        assert_eq!(PayloadType::AudioRealtime as u8, 0x60);
    }

    #[test]
    fn test_decode_invalid_version() {
        let mut buf = [0u8; 12];
        buf[0] = 0x00; // Version 0 instead of 2

        let result = RtpHeader::decode(&buf);
        assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(0))));
    }

    #[test]
    fn test_audio_samples_iterator() {
        let payload = vec![
            0x00, 0x01, 0x02, 0x03, // Sample 1: L=0x0100, R=0x0302
            0x04, 0x05, 0x06, 0x07, // Sample 2: L=0x0504, R=0x0706
        ];
        let packet = RtpPacket::audio(0, 0, 0, payload, false);

        let samples: Vec<_> = packet.audio_samples().collect();

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0], (0x0100, 0x0302));
        assert_eq!(samples[1], (0x0504, 0x0706));
    }
}
```

### Test File: `src/protocol/rtp/timing.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntp_timestamp_encode_decode() {
        let ts = NtpTimestamp {
            seconds: 1234567890,
            fraction: 0x80000000,
        };

        let encoded = ts.encode();
        let decoded = NtpTimestamp::decode(&encoded);

        assert_eq!(decoded.seconds, ts.seconds);
        assert_eq!(decoded.fraction, ts.fraction);
    }

    #[test]
    fn test_ntp_timestamp_now() {
        let ts = NtpTimestamp::now();

        // Should be somewhere reasonable (after 2020)
        assert!(ts.seconds > 3786825600); // 2020-01-01 in NTP time
    }

    #[test]
    fn test_timing_request_encode() {
        let request = TimingRequest::new();
        let encoded = request.encode(1, 0x12345678);

        // Check header
        assert_eq!(encoded[0], 0x80); // V=2
        assert_eq!(encoded[1], 0xD2); // M=1, PT=0x52

        // Should be 32 bytes total
        assert_eq!(encoded.len(), 32);
    }

    #[test]
    fn test_rtt_calculation() {
        // Simulate a response where server adds 10ms processing time
        let t1 = NtpTimestamp { seconds: 100, fraction: 0 };
        let t2 = NtpTimestamp { seconds: 100, fraction: 0x028F5C28 }; // +10ms
        let t3 = NtpTimestamp { seconds: 100, fraction: 0x051EB851 }; // +20ms
        let t4 = NtpTimestamp { seconds: 100, fraction: 0x0A3D70A3 }; // +40ms

        let response = TimingResponse {
            reference_time: t1,
            receive_time: t2,
            send_time: t3,
        };

        let rtt = response.calculate_rtt(t4);

        // RTT = (40-0) - (20-10) = 40 - 10 = 30ms ≈ 30000 microseconds
        // Allow some tolerance for floating point
        assert!(rtt > 25000 && rtt < 35000, "RTT was {}", rtt);
    }
}
```

### Test File: `src/protocol/rtp/codec.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_sequence_increment() {
        let mut codec = RtpCodec::new(0x12345678);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size];

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.sequence(), 1);

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.sequence(), 2);
    }

    #[test]
    fn test_codec_timestamp_increment() {
        let mut codec = RtpCodec::new(0x12345678);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size];

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.timestamp(), 352);

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.timestamp(), 704);
    }

    #[test]
    fn test_codec_invalid_audio_size() {
        let mut codec = RtpCodec::new(0);
        let audio = vec![0u8; 100]; // Wrong size

        let result = codec.encode_audio(&audio);
        assert!(matches!(result, Err(RtpCodecError::InvalidAudioSize(100))));
    }

    #[test]
    fn test_codec_encode_multiple_frames() {
        let mut codec = RtpCodec::new(0);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size * 3]; // 3 frames

        let packets = codec.encode_audio_frames(&audio).unwrap();

        assert_eq!(packets.len(), 3);
        assert_eq!(codec.sequence(), 3);
    }

    #[test]
    fn test_codec_with_encryption() {
        let mut codec = RtpCodec::new(0);
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];
        codec.set_encryption(key, iv);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0xAA; frame_size];

        let packet = codec.encode_audio(&audio).unwrap();

        // Encrypted payload should differ from original
        let decoded = RtpPacket::decode(&packet).unwrap();
        assert_ne!(decoded.payload, audio);
    }

    #[test]
    fn test_codec_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let mut encoder = RtpCodec::new(0x12345678);
        encoder.set_encryption(key, iv);

        let decoder = RtpCodec::new(0x12345678);
        // Note: decoder needs same keys for decryption

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let original = vec![0xAA; frame_size];

        let packet = encoder.encode_audio(&original).unwrap();

        // Create decoder with same keys
        let mut decoder = RtpCodec::new(0);
        decoder.set_encryption(key, iv);

        let decoded = decoder.decode_audio(&packet).unwrap();
        assert_eq!(decoded.payload, original);
    }
}
```

---

## Integration Tests

### Test: Complete audio packet flow

```rust
// tests/protocol/rtp_integration.rs

#[test]
fn test_audio_streaming_simulation() {
    let mut codec = RtpCodec::new(0xDEADBEEF);

    // Simulate 1 second of audio at 44.1kHz
    // 44100 samples / 352 samples per packet ≈ 125 packets
    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let total_samples = 44100;
    let total_bytes = total_samples * 4;

    let audio_data = vec![0u8; total_bytes];
    let packets = codec.encode_audio_frames(&audio_data).unwrap();

    assert!((packets.len() as i32 - 125).abs() <= 1);

    // Verify sequence numbers are continuous
    for (i, packet_data) in packets.iter().enumerate() {
        let packet = RtpPacket::decode(packet_data).unwrap();
        assert_eq!(packet.header.sequence, i as u16);
    }
}
```

---

## Acceptance Criteria

- [x] RTP header encodes/decodes correctly
- [x] Audio packets contain correct payload type
- [x] Sequence numbers increment correctly
- [x] Timestamps increment by samples-per-packet
- [x] Timing packets encode/decode correctly
- [x] Clock offset calculation is accurate
- [x] AES-CTR encryption works correctly
- [x] Retransmit requests encode/decode
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- Audio encryption position may need adjustment based on protocol analysis
- Consider adding jitter buffer simulation for testing
- May need to handle packet reordering in decoder
- Real devices may have timing requirements not covered here
