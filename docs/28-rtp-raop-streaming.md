# Section 28: RTP Audio Streaming for RAOP

## Dependencies
- **Section 06**: RTP/RAOP Protocol (must be complete)
- **Section 27**: RTSP Session for RAOP (must be complete)
- **Section 29**: RAOP Audio Encryption (recommended)

## Overview

RAOP uses RTP (Real-time Transport Protocol) for audio data transmission over UDP. Unlike standard RTP, RAOP includes Apple-specific extensions for synchronization, retransmission, and timing. Three UDP channels are used:

1. **Audio Channel**: Encrypted audio packets
2. **Control Channel**: Sync packets and retransmission requests
3. **Timing Channel**: NTP-style clock synchronization

## Objectives

- Extend RTP packet types for RAOP-specific formats
- Implement sync packet generation and parsing
- Implement timing protocol (NTP-style)
- Handle retransmission requests and responses
- Support packet loss detection and recovery

---

## Tasks

### 28.1 RAOP RTP Packet Types

- [ ] **28.1.1** Define RAOP-specific RTP payload types

**File:** `src/protocol/rtp/raop.rs`

```rust
//! RAOP-specific RTP packet types

use super::packet::{RtpHeader, RtpDecodeError};
use super::timing::NtpTimestamp;

/// RAOP RTP payload types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopPayloadType {
    /// Timing request (client -> server)
    TimingRequest = 0x52,
    /// Timing response (server -> client)
    TimingResponse = 0x53,
    /// Sync packet (server -> client on control channel)
    Sync = 0x54,
    /// Retransmit request (server -> client on control channel)
    RetransmitRequest = 0x55,
    /// Retransmit response (client -> server, audio data)
    RetransmitResponse = 0x56,
    /// Audio data (realtime mode)
    AudioRealtime = 0x60,
    /// Audio data (buffered mode)
    AudioBuffered = 0x61,
}

impl RaopPayloadType {
    /// Parse from byte value
    pub fn from_byte(b: u8) -> Option<Self> {
        match b & 0x7F {
            0x52 => Some(Self::TimingRequest),
            0x53 => Some(Self::TimingResponse),
            0x54 => Some(Self::Sync),
            0x55 => Some(Self::RetransmitRequest),
            0x56 => Some(Self::RetransmitResponse),
            0x60 => Some(Self::AudioRealtime),
            0x61 => Some(Self::AudioBuffered),
            _ => None,
        }
    }

    /// Check if this is an audio payload type
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::AudioRealtime | Self::AudioBuffered | Self::RetransmitResponse)
    }
}

/// RAOP sync packet (sent on control channel)
///
/// Provides synchronization between RTP timestamps and wall clock time.
#[derive(Debug, Clone)]
pub struct SyncPacket {
    /// Extension flag (set on first sync after RECORD/FLUSH)
    pub extension: bool,
    /// Current RTP timestamp being played
    pub rtp_timestamp: u32,
    /// Current NTP time
    pub ntp_time: NtpTimestamp,
    /// RTP timestamp of next audio packet
    pub next_timestamp: u32,
}

impl SyncPacket {
    /// Sync packet size (8-byte header + 4 + 8 + 4 = 24 bytes)
    pub const SIZE: usize = 20;

    /// Create a new sync packet
    pub fn new(
        rtp_timestamp: u32,
        ntp_time: NtpTimestamp,
        next_timestamp: u32,
        is_first: bool,
    ) -> Self {
        Self {
            extension: is_first,
            rtp_timestamp,
            ntp_time,
            next_timestamp,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);

        // RTP header (without SSRC)
        let flags = 0x80 | if self.extension { 0x10 } else { 0x00 };
        buf.push(flags);
        buf.push(0xD4); // Marker + PT=0x54

        // Sequence number (unused, set to 0x0007)
        buf.extend_from_slice(&0x0007u16.to_be_bytes());

        // RTP timestamp being played
        buf.extend_from_slice(&self.rtp_timestamp.to_be_bytes());

        // NTP timestamp (8 bytes)
        buf.extend_from_slice(&self.ntp_time.encode());

        // Next RTP timestamp
        buf.extend_from_slice(&self.next_timestamp.to_be_bytes());

        buf
    }

    /// Decode from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::SIZE,
                have: buf.len(),
            });
        }

        let extension = (buf[0] & 0x10) != 0;
        let rtp_timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ntp_time = NtpTimestamp::decode(&buf[8..16]);
        let next_timestamp = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);

        Ok(Self {
            extension,
            rtp_timestamp,
            ntp_time,
            next_timestamp,
        })
    }
}

/// Retransmit request packet
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub seq_start: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

impl RetransmitRequest {
    /// Packet size
    pub const SIZE: usize = 8;

    /// Decode from bytes (after 8-byte header)
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        Ok(Self {
            seq_start: u16::from_be_bytes([buf[0], buf[1]]),
            count: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }
}

/// RAOP audio packet with header
#[derive(Debug, Clone)]
pub struct RaopAudioPacket {
    /// Marker bit (set on first packet after RECORD/FLUSH)
    pub marker: bool,
    /// Sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// SSRC
    pub ssrc: u32,
    /// Audio payload (encrypted)
    pub payload: Vec<u8>,
}

impl RaopAudioPacket {
    /// RTP header size
    pub const HEADER_SIZE: usize = 12;

    /// Create a new audio packet
    pub fn new(sequence: u16, timestamp: u32, ssrc: u32, payload: Vec<u8>) -> Self {
        Self {
            marker: false,
            sequence,
            timestamp,
            ssrc,
            payload,
        }
    }

    /// Set marker bit (first packet after RECORD/FLUSH)
    pub fn with_marker(mut self) -> Self {
        self.marker = true;
        self
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::HEADER_SIZE + self.payload.len());

        // RTP header
        buf.push(0x80); // V=2, P=0, X=0, CC=0
        buf.push(0x60 | if self.marker { 0x80 } else { 0x00 }); // PT=0x60, M bit

        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        // Payload
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Decode from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::HEADER_SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::HEADER_SIZE,
                have: buf.len(),
            });
        }

        let marker = (buf[1] & 0x80) != 0;
        let sequence = u16::from_be_bytes([buf[2], buf[3]]);
        let timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ssrc = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let payload = buf[Self::HEADER_SIZE..].to_vec();

        Ok(Self {
            marker,
            sequence,
            timestamp,
            ssrc,
            payload,
        })
    }
}
```

---

### 28.2 Timing Protocol

- [ ] **28.2.1** Implement NTP-style timing exchange

**File:** `src/protocol/rtp/raop_timing.rs`

```rust
//! RAOP timing protocol implementation

use super::timing::NtpTimestamp;
use super::packet::RtpDecodeError;

/// Timing request packet (sent every 3 seconds)
#[derive(Debug, Clone)]
pub struct RaopTimingRequest {
    /// Reference time (when we sent this request)
    pub reference_time: NtpTimestamp,
}

impl RaopTimingRequest {
    /// Packet size
    pub const SIZE: usize = 32;

    /// Create new timing request
    pub fn new() -> Self {
        Self {
            reference_time: NtpTimestamp::now(),
        }
    }

    /// Encode to bytes
    pub fn encode(&self, sequence: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);

        // RTP header (no SSRC)
        buf.push(0x80); // V=2
        buf.push(0xD2); // M=1, PT=0x52

        buf.extend_from_slice(&sequence.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // Timestamp (unused)

        // Padding (4 bytes)
        buf.extend_from_slice(&[0u8; 4]);

        // Reference time
        buf.extend_from_slice(&self.reference_time.encode());

        // Receive time (0 for request)
        buf.extend_from_slice(&[0u8; 8]);

        // Send time (same as reference for request)
        buf.extend_from_slice(&self.reference_time.encode());

        buf
    }
}

/// Timing response packet (from server)
#[derive(Debug, Clone)]
pub struct RaopTimingResponse {
    /// Original reference time (from our request)
    pub reference_time: NtpTimestamp,
    /// Time server received our request
    pub receive_time: NtpTimestamp,
    /// Time server sent this response
    pub send_time: NtpTimestamp,
}

impl RaopTimingResponse {
    /// Decode from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 32 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 32,
                have: buf.len(),
            });
        }

        // Skip header (8 bytes)
        let reference_time = NtpTimestamp::decode(&buf[8..16]);
        let receive_time = NtpTimestamp::decode(&buf[16..24]);
        let send_time = NtpTimestamp::decode(&buf[24..32]);

        Ok(Self {
            reference_time,
            receive_time,
            send_time,
        })
    }

    /// Calculate clock offset (microseconds)
    ///
    /// offset = ((T2 - T1) + (T3 - T4)) / 2
    pub fn calculate_offset(&self, client_receive: NtpTimestamp) -> i64 {
        let t1 = self.reference_time.to_micros() as i64;
        let t2 = self.receive_time.to_micros() as i64;
        let t3 = self.send_time.to_micros() as i64;
        let t4 = client_receive.to_micros() as i64;

        ((t2 - t1) + (t3 - t4)) / 2
    }

    /// Calculate round-trip time (microseconds)
    ///
    /// RTT = (T4 - T1) - (T3 - T2)
    pub fn calculate_rtt(&self, client_receive: NtpTimestamp) -> u64 {
        let t1 = self.reference_time.to_micros();
        let t2 = self.receive_time.to_micros();
        let t3 = self.send_time.to_micros();
        let t4 = client_receive.to_micros();

        (t4 - t1).saturating_sub(t3 - t2)
    }
}

/// Timing synchronization manager
pub struct TimingSync {
    /// Sequence number for timing packets
    sequence: u16,
    /// Current clock offset (microseconds)
    offset: i64,
    /// Current RTT (microseconds)
    rtt: u64,
    /// Number of samples for averaging
    sample_count: u32,
    /// Last timing request sent
    last_request: Option<RaopTimingRequest>,
}

impl TimingSync {
    /// Create new timing sync manager
    pub fn new() -> Self {
        Self {
            sequence: 0,
            offset: 0,
            rtt: 0,
            sample_count: 0,
            last_request: None,
        }
    }

    /// Get current clock offset
    pub fn offset(&self) -> i64 {
        self.offset
    }

    /// Get current RTT
    pub fn rtt(&self) -> u64 {
        self.rtt
    }

    /// Create a timing request packet
    pub fn create_request(&mut self) -> Vec<u8> {
        let request = RaopTimingRequest::new();
        let data = request.encode(self.sequence);
        self.sequence = self.sequence.wrapping_add(1);
        self.last_request = Some(request);
        data
    }

    /// Process timing response
    pub fn process_response(&mut self, data: &[u8]) -> Result<(), RtpDecodeError> {
        let response = RaopTimingResponse::decode(data)?;
        let receive_time = NtpTimestamp::now();

        let offset = response.calculate_offset(receive_time);
        let rtt = response.calculate_rtt(receive_time);

        // Exponential moving average
        if self.sample_count == 0 {
            self.offset = offset;
            self.rtt = rtt;
        } else {
            // α = 0.125 (1/8) for smoothing
            self.offset = self.offset + (offset - self.offset) / 8;
            self.rtt = self.rtt + (rtt.saturating_sub(self.rtt)) / 8;
        }

        self.sample_count += 1;
        Ok(())
    }

    /// Convert local RTP timestamp to synchronized timestamp
    pub fn local_to_remote(&self, local_ts: u32) -> u32 {
        // Adjust by offset (converted to RTP timestamp units)
        let offset_samples = (self.offset * 44100 / 1_000_000) as i32;
        (local_ts as i32 + offset_samples) as u32
    }

    /// Convert remote RTP timestamp to local timestamp
    pub fn remote_to_local(&self, remote_ts: u32) -> u32 {
        let offset_samples = (self.offset * 44100 / 1_000_000) as i32;
        (remote_ts as i32 - offset_samples) as u32
    }
}
```

---

### 28.3 Audio Packet Buffer

- [ ] **28.3.1** Implement packet buffer for retransmission

**File:** `src/protocol/rtp/packet_buffer.rs`

```rust
//! Packet buffer for retransmission support

use std::collections::VecDeque;

/// Audio packet with sequence tracking
#[derive(Debug, Clone)]
pub struct BufferedPacket {
    /// Sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// Encoded packet data (ready for retransmission)
    pub data: Vec<u8>,
}

/// Circular buffer for recently sent packets
pub struct PacketBuffer {
    /// Maximum buffer size
    max_size: usize,
    /// Buffered packets
    packets: VecDeque<BufferedPacket>,
}

impl PacketBuffer {
    /// Default buffer size (1 second at ~125 packets/sec)
    pub const DEFAULT_SIZE: usize = 128;

    /// Create new packet buffer
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            packets: VecDeque::with_capacity(max_size),
        }
    }

    /// Add a packet to the buffer
    pub fn push(&mut self, packet: BufferedPacket) {
        if self.packets.len() >= self.max_size {
            self.packets.pop_front();
        }
        self.packets.push_back(packet);
    }

    /// Get a packet by sequence number
    pub fn get(&self, sequence: u16) -> Option<&BufferedPacket> {
        self.packets.iter().find(|p| p.sequence == sequence)
    }

    /// Get a range of packets for retransmission
    pub fn get_range(&self, start: u16, count: u16) -> Vec<&BufferedPacket> {
        let mut result = Vec::new();
        for seq in start..(start.wrapping_add(count)) {
            if let Some(packet) = self.get(seq) {
                result.push(packet);
            }
        }
        result
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.packets.clear();
    }

    /// Number of packets in buffer
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    /// Get sequence number range
    pub fn sequence_range(&self) -> Option<(u16, u16)> {
        if self.packets.is_empty() {
            None
        } else {
            Some((
                self.packets.front()?.sequence,
                self.packets.back()?.sequence,
            ))
        }
    }
}

/// Packet loss detector
pub struct PacketLossDetector {
    /// Expected next sequence number
    expected_seq: u16,
    /// First sequence received
    first_received: bool,
}

impl PacketLossDetector {
    /// Create new loss detector
    pub fn new() -> Self {
        Self {
            expected_seq: 0,
            first_received: false,
        }
    }

    /// Process received sequence number
    ///
    /// Returns list of missing sequence numbers
    pub fn process(&mut self, sequence: u16) -> Vec<u16> {
        if !self.first_received {
            self.first_received = true;
            self.expected_seq = sequence.wrapping_add(1);
            return Vec::new();
        }

        let mut missing = Vec::new();

        // Calculate how many packets were skipped
        let diff = sequence.wrapping_sub(self.expected_seq);

        if diff > 0 && diff < 100 {
            // Packets were lost
            for i in 0..diff {
                missing.push(self.expected_seq.wrapping_add(i));
            }
        }

        // Update expected
        self.expected_seq = sequence.wrapping_add(1);

        missing
    }

    /// Reset detector
    pub fn reset(&mut self) {
        self.first_received = false;
        self.expected_seq = 0;
    }
}
```

---

### 28.4 RAOP Audio Streamer

- [ ] **28.4.1** Implement audio streaming coordinator

**File:** `src/streaming/raop_streamer.rs`

```rust
//! RAOP audio streaming coordinator

use crate::protocol::rtp::raop::{RaopAudioPacket, SyncPacket};
use crate::protocol::rtp::packet_buffer::{PacketBuffer, BufferedPacket};
use crate::protocol::rtp::raop_timing::TimingSync;
use crate::protocol::raop::RaopSessionKeys;
use std::time::{Duration, Instant};

/// RAOP streaming configuration
#[derive(Debug, Clone)]
pub struct RaopStreamConfig {
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Samples per packet (352 for ALAC)
    pub samples_per_packet: u32,
    /// SSRC for RTP packets
    pub ssrc: u32,
    /// Enable retransmission buffer
    pub enable_retransmit: bool,
}

impl Default for RaopStreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            samples_per_packet: 352,
            ssrc: rand::random(),
            enable_retransmit: true,
        }
    }
}

/// RAOP audio streamer
pub struct RaopStreamer {
    /// Configuration
    config: RaopStreamConfig,
    /// Current sequence number
    sequence: u16,
    /// Current RTP timestamp
    timestamp: u32,
    /// Session encryption keys
    keys: RaopSessionKeys,
    /// Packet buffer for retransmission
    buffer: PacketBuffer,
    /// Timing synchronization
    timing: TimingSync,
    /// Is first packet after start/flush
    is_first_packet: bool,
    /// Last sync packet sent
    last_sync: Instant,
    /// Last timing request sent
    last_timing: Instant,
}

impl RaopStreamer {
    /// Timing request interval
    const TIMING_INTERVAL: Duration = Duration::from_secs(3);

    /// Sync packet interval
    const SYNC_INTERVAL: Duration = Duration::from_millis(1000);

    /// Create new streamer
    pub fn new(keys: RaopSessionKeys, config: RaopStreamConfig) -> Self {
        Self {
            config,
            sequence: 0,
            timestamp: 0,
            keys,
            buffer: PacketBuffer::new(PacketBuffer::DEFAULT_SIZE),
            timing: TimingSync::new(),
            is_first_packet: true,
            last_sync: Instant::now(),
            last_timing: Instant::now(),
        }
    }

    /// Get current sequence number
    pub fn sequence(&self) -> u16 {
        self.sequence
    }

    /// Get current timestamp
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Encode audio frame to RTP packet
    ///
    /// Audio should be encoded ALAC data (or raw PCM depending on codec)
    pub fn encode_frame(&mut self, audio_data: &[u8]) -> Vec<u8> {
        // Encrypt audio if keys are set
        let encrypted = self.encrypt_audio(audio_data);

        // Create packet
        let mut packet = RaopAudioPacket::new(
            self.sequence,
            self.timestamp,
            self.config.ssrc,
            encrypted,
        );

        if self.is_first_packet {
            packet = packet.with_marker();
            self.is_first_packet = false;
        }

        let encoded = packet.encode();

        // Buffer for retransmission
        if self.config.enable_retransmit {
            self.buffer.push(BufferedPacket {
                sequence: self.sequence,
                timestamp: self.timestamp,
                data: encoded.clone(),
            });
        }

        // Update state
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(self.config.samples_per_packet);

        encoded
    }

    fn encrypt_audio(&self, data: &[u8]) -> Vec<u8> {
        use crate::protocol::crypto::Aes128Ctr;

        let mut cipher = Aes128Ctr::new(
            self.keys.aes_key(),
            self.keys.aes_iv(),
        ).expect("invalid AES keys");

        // AES-CTR encryption
        let mut encrypted = data.to_vec();
        cipher.apply_keystream(&mut encrypted);
        encrypted
    }

    /// Handle retransmit request
    pub fn handle_retransmit(&self, seq_start: u16, count: u16) -> Vec<Vec<u8>> {
        self.buffer
            .get_range(seq_start, count)
            .into_iter()
            .map(|p| {
                // Wrap in retransmit response header
                let mut response = Vec::with_capacity(4 + p.data.len());
                response.push(0x80);
                response.push(0xD6); // PT=0x56 (retransmit response)
                response.extend_from_slice(&p.sequence.to_be_bytes());
                response.extend_from_slice(&p.data[4..]); // Skip original header
                response
            })
            .collect()
    }

    /// Check if sync packet should be sent
    pub fn should_send_sync(&self) -> bool {
        self.last_sync.elapsed() >= Self::SYNC_INTERVAL
    }

    /// Create sync packet
    pub fn create_sync_packet(&mut self) -> Vec<u8> {
        let ntp_time = crate::protocol::rtp::timing::NtpTimestamp::now();
        let packet = SyncPacket::new(
            self.timestamp,
            ntp_time,
            self.timestamp.wrapping_add(self.config.samples_per_packet),
            false,
        );
        self.last_sync = Instant::now();
        packet.encode()
    }

    /// Check if timing request should be sent
    pub fn should_send_timing(&self) -> bool {
        self.last_timing.elapsed() >= Self::TIMING_INTERVAL
    }

    /// Create timing request
    pub fn create_timing_request(&mut self) -> Vec<u8> {
        self.last_timing = Instant::now();
        self.timing.create_request()
    }

    /// Process timing response
    pub fn process_timing_response(&mut self, data: &[u8]) -> Result<(), String> {
        self.timing.process_response(data)
            .map_err(|e| e.to_string())
    }

    /// Flush and prepare for new playback
    pub fn flush(&mut self) {
        self.is_first_packet = true;
        self.buffer.clear();
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.sequence = 0;
        self.timestamp = 0;
        self.is_first_packet = true;
        self.buffer.clear();
        self.timing = TimingSync::new();
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/rtp/raop.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_packet_encode_decode() {
        let ntp = NtpTimestamp::now();
        let packet = SyncPacket::new(1000, ntp, 1352, true);

        let encoded = packet.encode();
        assert_eq!(encoded.len(), SyncPacket::SIZE);

        let decoded = SyncPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.rtp_timestamp, 1000);
        assert_eq!(decoded.next_timestamp, 1352);
        assert!(decoded.extension);
    }

    #[test]
    fn test_audio_packet_encode_decode() {
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let packet = RaopAudioPacket::new(100, 44100, 0x12345678, payload.clone())
            .with_marker();

        let encoded = packet.encode();
        let decoded = RaopAudioPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.timestamp, 44100);
        assert_eq!(decoded.ssrc, 0x12345678);
        assert!(decoded.marker);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_retransmit_request_decode() {
        let data = [0x00, 0x0A, 0x00, 0x05]; // seq=10, count=5
        let request = RetransmitRequest::decode(&data).unwrap();

        assert_eq!(request.seq_start, 10);
        assert_eq!(request.count, 5);
    }

    #[test]
    fn test_payload_type_parsing() {
        assert_eq!(RaopPayloadType::from_byte(0x60), Some(RaopPayloadType::AudioRealtime));
        assert_eq!(RaopPayloadType::from_byte(0xE0), Some(RaopPayloadType::AudioRealtime)); // With marker
        assert!(RaopPayloadType::AudioRealtime.is_audio());
        assert!(!RaopPayloadType::Sync.is_audio());
    }
}
```

### Test File: `src/protocol/rtp/packet_buffer.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_push_get() {
        let mut buffer = PacketBuffer::new(10);

        buffer.push(BufferedPacket {
            sequence: 100,
            timestamp: 0,
            data: vec![1, 2, 3],
        });

        let packet = buffer.get(100).unwrap();
        assert_eq!(packet.sequence, 100);
    }

    #[test]
    fn test_buffer_overflow() {
        let mut buffer = PacketBuffer::new(2);

        buffer.push(BufferedPacket { sequence: 1, timestamp: 0, data: vec![] });
        buffer.push(BufferedPacket { sequence: 2, timestamp: 0, data: vec![] });
        buffer.push(BufferedPacket { sequence: 3, timestamp: 0, data: vec![] });

        assert!(buffer.get(1).is_none()); // Evicted
        assert!(buffer.get(2).is_some());
        assert!(buffer.get(3).is_some());
    }

    #[test]
    fn test_buffer_range() {
        let mut buffer = PacketBuffer::new(10);

        for i in 0..5 {
            buffer.push(BufferedPacket {
                sequence: i,
                timestamp: i as u32 * 352,
                data: vec![i as u8],
            });
        }

        let range = buffer.get_range(1, 3);
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].sequence, 1);
        assert_eq!(range[2].sequence, 3);
    }

    #[test]
    fn test_loss_detector() {
        let mut detector = PacketLossDetector::new();

        // First packet
        let missing = detector.process(100);
        assert!(missing.is_empty());

        // Sequential
        let missing = detector.process(101);
        assert!(missing.is_empty());

        // Gap (102 missing)
        let missing = detector.process(103);
        assert_eq!(missing, vec![102]);

        // Larger gap
        let missing = detector.process(106);
        assert_eq!(missing, vec![104, 105]);
    }
}
```

---

## Integration Tests

### Test: Streaming simulation

```rust
// tests/raop_streaming_integration.rs

use airplay2_rs::streaming::raop_streamer::{RaopStreamer, RaopStreamConfig};
use airplay2_rs::protocol::raop::RaopSessionKeys;

#[test]
fn test_streaming_sequence() {
    // Create mock session keys
    let keys = create_test_keys();
    let config = RaopStreamConfig::default();

    let mut streamer = RaopStreamer::new(keys, config);

    // Simulate streaming audio frames
    let frame = vec![0u8; 352 * 4]; // 352 samples * 4 bytes (16-bit stereo)

    let packet1 = streamer.encode_frame(&frame);
    let packet2 = streamer.encode_frame(&frame);
    let packet3 = streamer.encode_frame(&frame);

    // Check sequence numbers
    assert_eq!(streamer.sequence(), 3);

    // Check timestamp progression
    assert_eq!(streamer.timestamp(), 352 * 3);

    // First packet should have marker bit
    assert_eq!(packet1[1] & 0x80, 0x80);
    // Subsequent packets should not
    assert_eq!(packet2[1] & 0x80, 0x00);
}

fn create_test_keys() -> RaopSessionKeys {
    // In tests, create mock keys
    todo!()
}
```

---

## Acceptance Criteria

- [ ] RAOP audio packets encode/decode correctly
- [ ] Sync packets include correct timestamps
- [ ] Timing protocol calculates offset accurately
- [ ] Packet buffer stores packets for retransmission
- [ ] Retransmit handler returns correct packets
- [ ] Marker bit set on first packet after flush
- [ ] Sequence numbers increment correctly
- [ ] Timestamps increment by samples-per-packet
- [ ] All unit tests pass
- [ ] Integration tests pass

---

## Notes

- Sync packets should be sent approximately once per second
- Timing packets should be sent every 3 seconds
- Packet buffer size should be tunable based on network conditions
- Consider adaptive bitrate based on packet loss
- Real-time constraints require careful scheduling
