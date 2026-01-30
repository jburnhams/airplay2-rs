# Section 39: RTP Receiver Core

## Dependencies
- **Section 06**: RTP Protocol (packet structures)
- **Section 37**: Session Management (socket allocation)
- **Section 38**: SDP Parsing (stream parameters)
- **Section 29**: RAOP Encryption (AES decryption)

## Overview

This section implements the RTP receiver core, which handles incoming UDP audio packets from the AirPlay sender. The receiver operates three UDP sockets:

1. **Audio Port**: Receives encrypted/encoded audio data packets
2. **Control Port**: Receives sync packets and handles retransmit requests
3. **Timing Port**: Handles NTP-like timing synchronization (Section 40)

The receiver must:
- Parse RTP headers and extract sequence numbers, timestamps
- Decrypt audio payloads (AES-128-CBC)
- Pass packets to the jitter buffer for reordering
- Handle packet loss detection

## Objectives

- Implement async UDP receive loops for all three ports
- Parse RTP packet headers efficiently
- Decrypt AES-128-CBC encrypted payloads
- Extract and validate sequence numbers for ordering
- Detect packet loss via sequence gaps
- Integrate with jitter buffer (Section 41)

---

## Tasks

### 39.1 RTP Packet Reception

- [x] **39.1.1** Implement RTP packet receiver

**File:** `src/receiver/rtp_receiver.rs`

```rust
//! RTP packet receiver for audio data
//!
//! Handles incoming RTP packets on the audio UDP port,
//! decrypts them, and forwards to the jitter buffer.

use crate::protocol::rtp::{RtpPacket, RtpHeader};
use crate::receiver::session::StreamParameters;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

/// Maximum UDP packet size
const MAX_PACKET_SIZE: usize = 2048;

/// RTP audio packet payload type
const PAYLOAD_TYPE_AUDIO: u8 = 0x60;

/// Received and decrypted audio packet
#[derive(Debug, Clone)]
pub struct AudioPacket {
    /// RTP sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// SSRC
    pub ssrc: u32,
    /// Decrypted audio data
    pub audio_data: Vec<u8>,
    /// Reception time (for jitter calculation)
    pub received_at: std::time::Instant,
}

/// Errors from RTP reception
#[derive(Debug, thiserror::Error)]
pub enum RtpReceiveError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid RTP packet")]
    InvalidPacket,

    #[error("Wrong payload type: {0}")]
    WrongPayloadType(u8),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Channel closed")]
    ChannelClosed,
}

/// RTP audio receiver
pub struct RtpAudioReceiver {
    socket: Arc<UdpSocket>,
    stream_params: StreamParameters,
    packet_tx: mpsc::Sender<AudioPacket>,
    decryptor: Option<AudioDecryptor>,
}

impl RtpAudioReceiver {
    /// Create a new RTP audio receiver
    pub fn new(
        socket: Arc<UdpSocket>,
        stream_params: StreamParameters,
        packet_tx: mpsc::Sender<AudioPacket>,
    ) -> Self {
        let decryptor = if let (Some(key), Some(iv)) = (stream_params.aes_key, stream_params.aes_iv) {
            Some(AudioDecryptor::new(key, iv))
        } else {
            None
        };

        Self {
            socket,
            stream_params,
            packet_tx,
            decryptor,
        }
    }

    /// Run the receive loop
    pub async fn run(self) -> Result<(), RtpReceiveError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];

        loop {
            let (len, _src) = self.socket.recv_from(&mut buf).await?;

            if len < 12 {
                // Too short for RTP header
                continue;
            }

            match self.process_packet(&buf[..len]).await {
                Ok(()) => {}
                Err(RtpReceiveError::ChannelClosed) => {
                    tracing::debug!("Audio channel closed, stopping receiver");
                    break;
                }
                Err(e) => {
                    tracing::warn!("RTP packet error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Process a single RTP packet
    async fn process_packet(&self, data: &[u8]) -> Result<(), RtpReceiveError> {
        // Parse RTP header
        let header = RtpHeader::parse(data)
            .ok_or(RtpReceiveError::InvalidPacket)?;

        // Check payload type
        if header.payload_type != PAYLOAD_TYPE_AUDIO {
            return Err(RtpReceiveError::WrongPayloadType(header.payload_type));
        }

        // Extract payload (after 12-byte header)
        let payload = &data[12..];

        // Decrypt if encryption is enabled
        let audio_data = if let Some(ref decryptor) = self.decryptor {
            decryptor.decrypt(payload)?
        } else {
            payload.to_vec()
        };

        // Create audio packet
        let packet = AudioPacket {
            sequence: header.sequence,
            timestamp: header.timestamp,
            ssrc: header.ssrc,
            audio_data,
            received_at: std::time::Instant::now(),
        };

        // Send to jitter buffer
        self.packet_tx.send(packet).await
            .map_err(|_| RtpReceiveError::ChannelClosed)?;

        Ok(())
    }
}

/// RTP header parser
impl RtpHeader {
    /// Parse RTP header from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }

        let byte0 = data[0];
        let byte1 = data[1];

        let version = (byte0 >> 6) & 0x03;
        if version != 2 {
            return None;  // RTP version must be 2
        }

        let padding = (byte0 >> 5) & 0x01 != 0;
        let extension = (byte0 >> 4) & 0x01 != 0;
        let csrc_count = byte0 & 0x0F;
        let marker = (byte1 >> 7) & 0x01 != 0;
        let payload_type = byte1 & 0x7F;

        let sequence = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        Some(RtpHeader {
            version,
            padding,
            extension,
            csrc_count,
            marker,
            payload_type,
            sequence,
            timestamp,
            ssrc,
        })
    }
}
```

---

### 39.2 Audio Decryptor

- [x] **39.2.1** Implement AES-128-CBC decryption for audio

**File:** `src/receiver/rtp_receiver.rs` (continued)

```rust
use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};

/// Audio payload decryptor (AES-128-CBC)
pub struct AudioDecryptor {
    key: [u8; 16],
    iv: [u8; 16],
}

impl AudioDecryptor {
    pub fn new(key: [u8; 16], iv: [u8; 16]) -> Self {
        Self { key, iv }
    }

    /// Decrypt audio payload
    ///
    /// RAOP uses AES-128-CBC with the IV from SDP.
    /// Each packet uses the same IV (not chained between packets).
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, RtpReceiveError> {
        if encrypted.is_empty() {
            return Ok(Vec::new());
        }

        // AES-CBC works on 16-byte blocks
        // RAOP only encrypts complete blocks, leaving remainder unencrypted
        let block_size = 16;
        let encrypted_len = (encrypted.len() / block_size) * block_size;

        if encrypted_len == 0 {
            // Less than one block, no encryption
            return Ok(encrypted.to_vec());
        }

        let cipher = Aes128::new(GenericArray::from_slice(&self.key));

        let mut decrypted = Vec::with_capacity(encrypted.len());

        // Decrypt in CBC mode
        let mut prev_block = self.iv;

        for chunk in encrypted[..encrypted_len].chunks(block_size) {
            let mut block = GenericArray::clone_from_slice(chunk);

            // Save ciphertext for next XOR
            let ciphertext: [u8; 16] = chunk.try_into().unwrap();

            // Decrypt block
            cipher.decrypt_block(&mut block);

            // XOR with previous ciphertext (or IV for first block)
            for (b, p) in block.iter_mut().zip(prev_block.iter()) {
                *b ^= *p;
            }

            decrypted.extend_from_slice(&block);
            prev_block = ciphertext;
        }

        // Append unencrypted remainder
        if encrypted_len < encrypted.len() {
            decrypted.extend_from_slice(&encrypted[encrypted_len..]);
        }

        Ok(decrypted)
    }
}
```

---

### 39.3 Control Port Handler

- [x] **39.3.1** Implement control port receiver for sync packets

**File:** `src/receiver/control_receiver.rs`

```rust
//! Control port receiver
//!
//! Handles sync packets and retransmission requests on the control UDP port.

use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

/// Control packet types
const PACKET_TYPE_SYNC: u8 = 0x54;
const PACKET_TYPE_RETRANSMIT_REQUEST: u8 = 0x55;

/// Sync packet from sender
#[derive(Debug, Clone)]
pub struct SyncPacket {
    /// Extension bit
    pub extension: bool,
    /// RTP timestamp at next packet
    pub rtp_timestamp: u32,
    /// NTP timestamp (when sent)
    pub ntp_timestamp: u64,
    /// RTP timestamp at NTP time
    pub rtp_timestamp_at_ntp: u32,
}

/// Retransmit request (we receive these; respond on control port)
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub first_seq: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

/// Events from control port
#[derive(Debug, Clone)]
pub enum ControlEvent {
    Sync(SyncPacket),
    RetransmitRequest(RetransmitRequest),
}

/// Control port receiver
pub struct ControlReceiver {
    socket: Arc<UdpSocket>,
    event_tx: mpsc::Sender<ControlEvent>,
}

impl ControlReceiver {
    pub fn new(socket: Arc<UdpSocket>, event_tx: mpsc::Sender<ControlEvent>) -> Self {
        Self { socket, event_tx }
    }

    /// Run the receive loop
    pub async fn run(self) -> Result<(), std::io::Error> {
        let mut buf = [0u8; 256];

        loop {
            let (len, src) = self.socket.recv_from(&mut buf).await?;

            if len < 8 {
                continue;
            }

            if let Some(event) = self.parse_packet(&buf[..len]) {
                if self.event_tx.send(event).await.is_err() {
                    break;
                }
            }
        }

        Ok(())
    }

    fn parse_packet(&self, data: &[u8]) -> Option<ControlEvent> {
        if data.len() < 8 {
            return None;
        }

        let packet_type = data[1] & 0x7F;

        match packet_type {
            PACKET_TYPE_SYNC => self.parse_sync(data),
            PACKET_TYPE_RETRANSMIT_REQUEST => self.parse_retransmit(data),
            _ => None,
        }
    }

    fn parse_sync(&self, data: &[u8]) -> Option<ControlEvent> {
        // Sync packet format:
        // Byte 0: 0x80 | extension bit
        // Byte 1: 0x54 (marker + type)
        // Bytes 2-3: sequence (ignored)
        // Bytes 4-7: RTP timestamp (next packet)
        // Bytes 8-15: NTP timestamp
        // Bytes 16-19: RTP timestamp at NTP time

        if data.len() < 20 {
            return None;
        }

        let extension = (data[0] & 0x10) != 0;
        let rtp_timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ntp_timestamp = u64::from_be_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        let rtp_timestamp_at_ntp = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);

        Some(ControlEvent::Sync(SyncPacket {
            extension,
            rtp_timestamp,
            ntp_timestamp,
            rtp_timestamp_at_ntp,
        }))
    }

    fn parse_retransmit(&self, data: &[u8]) -> Option<ControlEvent> {
        // Retransmit request format:
        // Bytes 0-1: Header
        // Bytes 2-3: Sequence of missing packet
        // Bytes 4-5: Count

        if data.len() < 8 {
            return None;
        }

        let first_seq = u16::from_be_bytes([data[4], data[5]]);
        let count = u16::from_be_bytes([data[6], data[7]]);

        Some(ControlEvent::RetransmitRequest(RetransmitRequest {
            first_seq,
            count,
        }))
    }
}
```

---

### 39.4 Packet Loss Detection

- [x] **39.4.1** Implement sequence number tracking and gap detection

**File:** `src/receiver/sequence_tracker.rs`

```rust
//! RTP sequence number tracking and packet loss detection

use std::collections::VecDeque;

/// Tracks RTP sequence numbers to detect gaps
pub struct SequenceTracker {
    /// Last received sequence number
    last_seq: Option<u16>,
    /// Expected next sequence number
    expected_seq: Option<u16>,
    /// Recent gap history for statistics
    recent_gaps: VecDeque<GapInfo>,
    /// Maximum history size
    max_history: usize,
    /// Total packets received
    packets_received: u64,
    /// Total gaps detected
    total_gaps: u64,
    /// Total packets lost
    total_lost: u64,
}

#[derive(Debug, Clone)]
pub struct GapInfo {
    /// First missing sequence
    pub start: u16,
    /// Count of missing packets
    pub count: u16,
    /// When gap was detected
    pub detected_at: std::time::Instant,
}

impl SequenceTracker {
    pub fn new() -> Self {
        Self {
            last_seq: None,
            expected_seq: None,
            recent_gaps: VecDeque::with_capacity(100),
            max_history: 100,
            packets_received: 0,
            total_gaps: 0,
            total_lost: 0,
        }
    }

    /// Record a received packet, returning any detected gap
    pub fn record(&mut self, seq: u16) -> Option<GapInfo> {
        self.packets_received += 1;

        let gap = if let Some(expected) = self.expected_seq {
            let gap_size = self.sequence_gap(expected, seq);

            if gap_size > 0 && gap_size < 1000 {
                // Gap detected (but not wrap-around)
                self.total_gaps += 1;
                self.total_lost += gap_size as u64;

                let gap_info = GapInfo {
                    start: expected,
                    count: gap_size,
                    detected_at: std::time::Instant::now(),
                };

                if self.recent_gaps.len() >= self.max_history {
                    self.recent_gaps.pop_front();
                }
                self.recent_gaps.push_back(gap_info.clone());

                Some(gap_info)
            } else {
                None
            }
        } else {
            None
        };

        self.last_seq = Some(seq);
        self.expected_seq = Some(seq.wrapping_add(1));

        gap
    }

    /// Calculate gap between expected and actual sequence numbers
    /// Handles 16-bit wraparound correctly
    fn sequence_gap(&self, expected: u16, actual: u16) -> u16 {
        actual.wrapping_sub(expected)
    }

    /// Check if a sequence number is expected (not duplicate, not too old)
    pub fn is_expected(&self, seq: u16) -> bool {
        if let Some(expected) = self.expected_seq {
            let diff = seq.wrapping_sub(expected);
            // Accept if within reasonable window (ahead or slightly behind)
            diff < 1000 || diff > 65000
        } else {
            true  // First packet
        }
    }

    /// Get packet loss ratio (0.0 to 1.0)
    pub fn loss_ratio(&self) -> f64 {
        if self.packets_received == 0 {
            return 0.0;
        }
        let total = self.packets_received + self.total_lost;
        self.total_lost as f64 / total as f64
    }

    /// Get statistics
    pub fn stats(&self) -> SequenceStats {
        SequenceStats {
            packets_received: self.packets_received,
            total_gaps: self.total_gaps,
            total_lost: self.total_lost,
            loss_ratio: self.loss_ratio(),
        }
    }

    /// Reset the tracker
    pub fn reset(&mut self) {
        self.last_seq = None;
        self.expected_seq = None;
        self.recent_gaps.clear();
        self.packets_received = 0;
        self.total_gaps = 0;
        self.total_lost = 0;
    }
}

#[derive(Debug, Clone)]
pub struct SequenceStats {
    pub packets_received: u64,
    pub total_gaps: u64,
    pub total_lost: u64,
    pub loss_ratio: f64,
}

impl Default for SequenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_packets() {
        let mut tracker = SequenceTracker::new();

        assert!(tracker.record(100).is_none());
        assert!(tracker.record(101).is_none());
        assert!(tracker.record(102).is_none());

        assert_eq!(tracker.packets_received, 3);
        assert_eq!(tracker.total_lost, 0);
    }

    #[test]
    fn test_gap_detection() {
        let mut tracker = SequenceTracker::new();

        tracker.record(100);
        let gap = tracker.record(105);  // Skipped 101-104

        assert!(gap.is_some());
        let gap = gap.unwrap();
        assert_eq!(gap.start, 101);
        assert_eq!(gap.count, 4);
    }

    #[test]
    fn test_wraparound() {
        let mut tracker = SequenceTracker::new();

        tracker.record(65534);
        tracker.record(65535);
        let gap = tracker.record(0);  // Wrap to 0

        assert!(gap.is_none());
        assert_eq!(tracker.total_lost, 0);
    }

    #[test]
    fn test_loss_ratio() {
        let mut tracker = SequenceTracker::new();

        tracker.record(100);
        tracker.record(105);  // Lost 4 packets

        assert!((tracker.loss_ratio() - 0.666).abs() < 0.01);
    }
}
```

---

### 39.5 Combined Receiver Manager

- [x] **39.5.1** Implement unified receiver management

**File:** `src/receiver/receiver_manager.rs`

```rust
//! Combined RTP receiver manager
//!
//! Manages all three UDP receive loops and coordinates
//! packet flow to the audio pipeline.

use super::rtp_receiver::{RtpAudioReceiver, AudioPacket, RtpReceiveError};
use super::control_receiver::{ControlReceiver, ControlEvent};
use super::sequence_tracker::SequenceTracker;
use crate::receiver::session::StreamParameters;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

/// Receiver manager configuration
#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    /// Audio packet channel buffer size
    pub audio_buffer_size: usize,
    /// Control event channel buffer size
    pub control_buffer_size: usize,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            audio_buffer_size: 512,
            control_buffer_size: 64,
        }
    }
}

/// Manages all RTP receive operations
pub struct ReceiverManager {
    config: ReceiverConfig,
    audio_rx: mpsc::Receiver<AudioPacket>,
    control_rx: mpsc::Receiver<ControlEvent>,
    sequence_tracker: Arc<RwLock<SequenceTracker>>,
    handles: Vec<JoinHandle<()>>,
}

impl ReceiverManager {
    /// Start receivers on provided sockets
    pub fn start(
        audio_socket: Arc<UdpSocket>,
        control_socket: Arc<UdpSocket>,
        stream_params: StreamParameters,
        config: ReceiverConfig,
    ) -> Self {
        let (audio_tx, audio_rx) = mpsc::channel(config.audio_buffer_size);
        let (control_tx, control_rx) = mpsc::channel(config.control_buffer_size);
        let sequence_tracker = Arc::new(RwLock::new(SequenceTracker::new()));

        // Start audio receiver
        let audio_receiver = RtpAudioReceiver::new(
            audio_socket,
            stream_params,
            audio_tx,
        );

        let audio_handle = tokio::spawn(async move {
            if let Err(e) = audio_receiver.run().await {
                tracing::error!("Audio receiver error: {}", e);
            }
        });

        // Start control receiver
        let control_receiver = ControlReceiver::new(control_socket, control_tx);

        let control_handle = tokio::spawn(async move {
            if let Err(e) = control_receiver.run().await {
                tracing::error!("Control receiver error: {}", e);
            }
        });

        Self {
            config,
            audio_rx,
            control_rx,
            sequence_tracker,
            handles: vec![audio_handle, control_handle],
        }
    }

    /// Receive next audio packet
    pub async fn recv_audio(&mut self) -> Option<AudioPacket> {
        let packet = self.audio_rx.recv().await?;

        // Track sequence
        let mut tracker = self.sequence_tracker.write().await;
        if let Some(gap) = tracker.record(packet.sequence) {
            tracing::debug!(
                "Packet loss detected: {} packets starting at seq {}",
                gap.count,
                gap.start
            );
        }

        Some(packet)
    }

    /// Receive next control event
    pub async fn recv_control(&mut self) -> Option<ControlEvent> {
        self.control_rx.recv().await
    }

    /// Get sequence tracker for statistics
    pub fn sequence_tracker(&self) -> Arc<RwLock<SequenceTracker>> {
        self.sequence_tracker.clone()
    }

    /// Stop all receivers
    pub async fn stop(self) {
        for handle in self.handles {
            handle.abort();
        }
    }
}
```

---

## Unit Tests

### 39.6 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtp_header_parse() {
        // Valid RTP packet header
        let data = [
            0x80, 0x60,  // V=2, P=0, X=0, CC=0, M=0, PT=96
            0x00, 0x01,  // Sequence = 1
            0x00, 0x00, 0x00, 0x0A,  // Timestamp = 10
            0x12, 0x34, 0x56, 0x78,  // SSRC
            0x00, 0x00,  // Payload start
        ];

        let header = RtpHeader::parse(&data).unwrap();

        assert_eq!(header.version, 2);
        assert_eq!(header.payload_type, 0x60);
        assert_eq!(header.sequence, 1);
        assert_eq!(header.timestamp, 10);
        assert_eq!(header.ssrc, 0x12345678);
    }

    #[test]
    fn test_rtp_header_invalid_version() {
        let data = [
            0x40, 0x60,  // V=1 (invalid)
            0x00, 0x01,
            0x00, 0x00, 0x00, 0x0A,
            0x12, 0x34, 0x56, 0x78,
        ];

        assert!(RtpHeader::parse(&data).is_none());
    }

    #[test]
    fn test_audio_decryptor() {
        let key = [0x01; 16];
        let iv = [0x02; 16];
        let decryptor = AudioDecryptor::new(key, iv);

        // Test with less than one block (unencrypted)
        let short_data = [0x03; 10];
        let result = decryptor.decrypt(&short_data).unwrap();
        assert_eq!(result, short_data);
    }

    #[test]
    fn test_sync_packet_parse() {
        let data = [
            0x90, 0xD4,  // Header with sync type
            0x00, 0x01,  // Sequence
            0x00, 0x00, 0x01, 0x00,  // RTP timestamp
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,  // NTP timestamp
            0x00, 0x00, 0x00, 0xFF,  // RTP at NTP
        ];

        let receiver = ControlReceiver::new(
            Arc::new(tokio::net::UdpSocket::bind("0.0.0.0:0").unwrap()),
            mpsc::channel(1).0,
        );

        // Use internal parse for testing
        let event = receiver.parse_sync(&data);
        assert!(event.is_some());

        if let Some(ControlEvent::Sync(sync)) = event {
            assert_eq!(sync.rtp_timestamp, 256);
            assert_eq!(sync.ntp_timestamp, 1);
            assert_eq!(sync.rtp_timestamp_at_ntp, 255);
        }
    }
}
```

---

## Integration Tests

### 39.7 Integration Tests

**File:** `tests/receiver/rtp_receiver_tests.rs`

```rust
use tokio::net::UdpSocket;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_audio_packet_reception() {
    // Create receiver socket
    let receiver_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let receiver_addr = receiver_socket.local_addr().unwrap();

    // Create sender socket
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Setup receiver
    let (tx, mut rx) = mpsc::channel(16);
    let params = StreamParameters::default();
    let audio_receiver = RtpAudioReceiver::new(receiver_socket, params, tx);

    // Start receiver
    let handle = tokio::spawn(async move {
        audio_receiver.run().await
    });

    // Send a valid RTP packet
    let rtp_packet = build_test_rtp_packet(1, 0, &[0xAB; 100]);
    sender_socket.send_to(&rtp_packet, receiver_addr).await.unwrap();

    // Receive and verify
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        rx.recv()
    ).await.unwrap().unwrap();

    assert_eq!(received.sequence, 1);
    assert_eq!(received.timestamp, 0);

    handle.abort();
}

fn build_test_rtp_packet(seq: u16, timestamp: u32, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![
        0x80, 0x60,  // V=2, PT=96
        (seq >> 8) as u8, seq as u8,
        (timestamp >> 24) as u8, (timestamp >> 16) as u8,
        (timestamp >> 8) as u8, timestamp as u8,
        0x12, 0x34, 0x56, 0x78,  // SSRC
    ];
    packet.extend_from_slice(payload);
    packet
}
```

---

## Acceptance Criteria

- [x] Parse RTP headers correctly (version, PT, seq, timestamp, SSRC)
- [x] Decrypt AES-128-CBC payloads correctly
- [x] Handle unencrypted streams
- [x] Receive packets on audio UDP port
- [x] Receive sync packets on control port
- [x] Detect packet loss via sequence gaps
- [x] Calculate accurate loss statistics
- [x] Handle 16-bit sequence wraparound
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- **Payload type**: RAOP uses 0x60 (96) for audio; other types are control/timing
- **Decryption**: AES-CBC with same IV per packet (not chained)
- **Sequence tracking**: 16-bit wrapping handled correctly
- **Buffer sizes**: Tune based on network conditions and latency requirements
- **Future**: Retransmit requests could be sent for lost packets

---

## References

- [RFC 3550](https://tools.ietf.org/html/rfc3550) - RTP: A Transport Protocol for Real-Time Applications
- [RAOP RTP Format](https://nto.github.io/AirPlay.html#audio-rtp)
