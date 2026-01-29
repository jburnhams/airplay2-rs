# Section 61: Testing Infrastructure

## Dependencies
- **Section 60**: Receiver Integration
- **Section 19**: Mock Server (existing patterns)

## Overview

This section establishes the testing infrastructure for the AirPlay 2 receiver. Testing strategy:

1. **Unit Tests** - Individual component testing with mocks
2. **Mock Sender** - Simulated iOS/macOS client for integration tests
3. **Packet Captures** - Real traffic recordings for replay tests
4. **Conformance Tests** - Protocol specification validation

No real device testing at this phase - all tests use simulated senders and captured traffic.

## Objectives

- Create mock AirPlay 2 sender
- Build packet capture replay infrastructure
- Establish test data management
- Define test utilities and helpers
- Enable CI/CD testing without hardware

---

## Tasks

### 61.1 Mock AirPlay 2 Sender

**File:** `src/testing/mock_ap2_sender.rs`

```rust
//! Mock AirPlay 2 Sender for Testing
//!
//! Simulates an iOS/macOS device connecting to our receiver,
//! performing pairing, and streaming audio.

use crate::protocol::rtsp::{RtspRequest, Method, Headers};
use crate::protocol::plist::PlistValue;
use crate::protocol::pairing::tlv::TlvEncoder;
use crate::protocol::crypto::{
    srp::SrpClient,
    ed25519::Ed25519Keypair,
    x25519::X25519Keypair,
    chacha::ChaCha20Poly1305,
};
use crate::receiver::ap2::body_handler::encode_bplist_body;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Mock sender configuration
#[derive(Debug, Clone)]
pub struct MockSenderConfig {
    /// Sender name
    pub name: String,
    /// PIN/password to use for pairing
    pub pin: String,
    /// Audio format to request
    pub audio_format: MockAudioFormat,
    /// Enable encryption
    pub encrypt: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum MockAudioFormat {
    Pcm44100,
    Alac44100,
    AacEld,
}

impl Default for MockSenderConfig {
    fn default() -> Self {
        Self {
            name: "MockSender".to_string(),
            pin: "1234".to_string(),
            audio_format: MockAudioFormat::Pcm44100,
            encrypt: true,
        }
    }
}

/// Mock AirPlay 2 sender for testing
pub struct MockAp2Sender {
    config: MockSenderConfig,
    stream: Option<TcpStream>,
    cseq: u32,
    session_id: Option<String>,
    identity: Ed25519Keypair,
    encryption_key: Option<[u8; 32]>,
}

impl MockAp2Sender {
    /// Create a new mock sender
    pub fn new(config: MockSenderConfig) -> Self {
        Self {
            config,
            stream: None,
            cseq: 0,
            session_id: None,
            identity: Ed25519Keypair::generate(),
            encryption_key: None,
        }
    }

    /// Connect to a receiver
    pub async fn connect(&mut self, addr: SocketAddr) -> Result<(), MockSenderError> {
        self.stream = Some(TcpStream::connect(addr).await?);
        log::debug!("Mock sender connected to {}", addr);
        Ok(())
    }

    /// Perform full session: info -> pairing -> setup -> record
    pub async fn full_session(&mut self) -> Result<MockSessionResult, MockSenderError> {
        // Step 1: GET /info
        let info = self.get_info().await?;
        log::debug!("Received device info");

        // Step 2: Pair-setup
        self.pair_setup().await?;
        log::debug!("Pairing setup complete");

        // Step 3: Pair-verify
        let encryption_key = self.pair_verify().await?;
        self.encryption_key = Some(encryption_key);
        log::debug!("Pairing verify complete, encryption enabled");

        // Step 4: SETUP phase 1 (timing)
        let timing_ports = self.setup_timing().await?;
        log::debug!("Setup phase 1 complete: {:?}", timing_ports);

        // Step 5: SETUP phase 2 (audio)
        let audio_ports = self.setup_audio().await?;
        log::debug!("Setup phase 2 complete: {:?}", audio_ports);

        // Step 6: RECORD
        self.record().await?;
        log::debug!("Recording started");

        Ok(MockSessionResult {
            timing_port: timing_ports.0,
            audio_data_port: audio_ports.0,
            audio_control_port: audio_ports.1,
        })
    }

    /// GET /info request
    pub async fn get_info(&mut self) -> Result<PlistValue, MockSenderError> {
        let request = self.build_request(Method::Get, "/info", None);
        let response = self.send_request(&request).await?;
        // Parse response body as plist
        Ok(PlistValue::Dict(HashMap::new()))  // Simplified
    }

    /// Perform pair-setup (M1-M4)
    pub async fn pair_setup(&mut self) -> Result<(), MockSenderError> {
        // M1: Send method and state
        let m1 = TlvEncoder::new()
            .add_u8(0x06, 1)  // State = 1
            .add_u8(0x00, 0)  // Method = pair-setup
            .encode();

        let request = self.build_request(Method::Post, "/pair-setup", Some(m1));
        let _response = self.send_request(&request).await?;

        // Parse M2, compute M3, etc.
        // (Simplified - real implementation would complete SRP)

        Ok(())
    }

    /// Perform pair-verify (M1-M4)
    pub async fn pair_verify(&mut self) -> Result<[u8; 32], MockSenderError> {
        let keypair = X25519Keypair::generate();

        // M1: Send our public key
        let m1 = TlvEncoder::new()
            .add_u8(0x06, 1)  // State = 1
            .add_bytes(0x03, keypair.public_key().as_bytes())
            .encode();

        let request = self.build_request(Method::Post, "/pair-verify", Some(m1));
        let _response = self.send_request(&request).await?;

        // Complete verify exchange...
        // Return derived encryption key
        Ok([0u8; 32])  // Placeholder
    }

    /// SETUP phase 1 (timing)
    pub async fn setup_timing(&mut self) -> Result<(u16, u16), MockSenderError> {
        let mut streams = HashMap::new();
        streams.insert("type".to_string(), PlistValue::Integer(150)); // Timing

        let body = encode_bplist_body(&PlistValue::Dict({
            let mut d = HashMap::new();
            d.insert("streams".to_string(), PlistValue::Array(vec![PlistValue::Dict(streams)]));
            d.insert("timingProtocol".to_string(), PlistValue::String("PTP".into()));
            d
        })).map_err(|e| MockSenderError::Protocol(e.to_string()))?;

        let request = self.build_request(Method::Setup, "/setup", Some(body));
        let _response = self.send_request(&request).await?;

        Ok((7011, 7010))  // Placeholder ports
    }

    /// SETUP phase 2 (audio)
    pub async fn setup_audio(&mut self) -> Result<(u16, u16), MockSenderError> {
        let mut streams = HashMap::new();
        streams.insert("type".to_string(), PlistValue::Integer(96)); // Audio
        streams.insert("ct".to_string(), PlistValue::Integer(100));  // PCM
        streams.insert("sr".to_string(), PlistValue::Integer(44100));
        streams.insert("ch".to_string(), PlistValue::Integer(2));
        streams.insert("ss".to_string(), PlistValue::Integer(16));

        let body = encode_bplist_body(&PlistValue::Dict({
            let mut d = HashMap::new();
            d.insert("streams".to_string(), PlistValue::Array(vec![PlistValue::Dict(streams)]));
            d
        })).map_err(|e| MockSenderError::Protocol(e.to_string()))?;

        let request = self.build_request(Method::Setup, "/setup", Some(body));
        let _response = self.send_request(&request).await?;

        Ok((7100, 7101))  // Placeholder ports
    }

    /// Send RECORD
    pub async fn record(&mut self) -> Result<(), MockSenderError> {
        let request = self.build_request(Method::Record, "/record", None);
        let _response = self.send_request(&request).await?;
        Ok(())
    }

    /// Send audio packet
    pub async fn send_audio(&self, _samples: &[i16], _timestamp: u32) -> Result<(), MockSenderError> {
        // Would send encrypted RTP packet
        Ok(())
    }

    /// Send TEARDOWN
    pub async fn teardown(&mut self) -> Result<(), MockSenderError> {
        let request = self.build_request(Method::Teardown, "/teardown", None);
        let _response = self.send_request(&request).await?;
        Ok(())
    }

    fn build_request(&mut self, method: Method, uri: &str, body: Option<Vec<u8>>) -> RtspRequest {
        self.cseq += 1;

        let mut headers = Headers::new();
        headers.insert("CSeq".to_string(), self.cseq.to_string());
        headers.insert("User-Agent".to_string(), "MockSender/1.0".to_string());

        if let Some(ref session) = self.session_id {
            headers.insert("Session".to_string(), session.clone());
        }

        if let Some(ref b) = body {
            headers.insert("Content-Length".to_string(), b.len().to_string());
            headers.insert("Content-Type".to_string(), "application/x-apple-binary-plist".to_string());
        }

        RtspRequest {
            method,
            uri: uri.to_string(),
            headers,
            body: body.unwrap_or_default(),
        }
    }

    async fn send_request(&mut self, request: &RtspRequest) -> Result<Vec<u8>, MockSenderError> {
        let stream = self.stream.as_mut()
            .ok_or(MockSenderError::NotConnected)?;

        // Serialize and send request
        let request_bytes = Self::serialize_request(request);

        // Optionally encrypt if key is set
        let to_send = if let Some(_key) = self.encryption_key {
            // Encrypt with HAP framing
            request_bytes  // Simplified
        } else {
            request_bytes
        };

        stream.write_all(&to_send).await?;

        // Read response
        let mut response = vec![0u8; 4096];
        let n = stream.read(&mut response).await?;
        response.truncate(n);

        Ok(response)
    }

    fn serialize_request(request: &RtspRequest) -> Vec<u8> {
        let mut output = Vec::new();

        // Request line
        output.extend_from_slice(
            format!("{} {} RTSP/1.0\r\n", request.method.as_str(), request.uri).as_bytes()
        );

        // Headers
        for (name, value) in request.headers.iter() {
            output.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        output.extend_from_slice(b"\r\n");

        // Body
        output.extend_from_slice(&request.body);

        output
    }
}

/// Result of a mock session
#[derive(Debug)]
pub struct MockSessionResult {
    pub timing_port: u16,
    pub audio_data_port: u16,
    pub audio_control_port: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum MockSenderError {
    #[error("Not connected")]
    NotConnected,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Pairing failed: {0}")]
    PairingFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_sender_creation() {
        let sender = MockAp2Sender::new(MockSenderConfig::default());
        assert!(sender.stream.is_none());
    }

    #[test]
    fn test_request_building() {
        let mut sender = MockAp2Sender::new(MockSenderConfig::default());
        let request = sender.build_request(Method::Options, "*", None);

        assert_eq!(request.method, Method::Options);
        assert_eq!(request.uri, "*");
        assert!(request.headers.cseq().is_some());
    }
}
```

---

### 61.2 Packet Capture Infrastructure

**File:** `src/testing/packet_capture.rs`

```rust
//! Packet Capture Replay for Testing
//!
//! Allows replaying captured AirPlay traffic for testing.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::Duration;

/// Captured packet
#[derive(Debug, Clone)]
pub struct CapturedPacket {
    /// Timestamp offset from start (microseconds)
    pub timestamp_us: u64,
    /// Direction (true = sender -> receiver)
    pub inbound: bool,
    /// Protocol (TCP, UDP)
    pub protocol: CaptureProtocol,
    /// Packet data
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProtocol {
    Tcp,
    Udp,
}

/// Capture file loader
pub struct CaptureLoader;

impl CaptureLoader {
    /// Load capture from hex dump file
    ///
    /// Format: `timestamp_us direction protocol hex_data`
    pub fn load_hex_dump(path: &Path) -> Result<Vec<CapturedPacket>, CaptureError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut packets = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            let timestamp_us: u64 = parts[0].parse()
                .map_err(|_| CaptureError::InvalidFormat)?;
            let inbound = parts[1] == "IN";
            let protocol = match parts[2] {
                "TCP" => CaptureProtocol::Tcp,
                "UDP" => CaptureProtocol::Udp,
                _ => continue,
            };
            let data = hex::decode(parts[3])
                .map_err(|_| CaptureError::InvalidHex)?;

            packets.push(CapturedPacket {
                timestamp_us,
                inbound,
                protocol,
                data,
            });
        }

        Ok(packets)
    }

    /// Load capture from pcap file (simplified)
    pub fn load_pcap(path: &Path) -> Result<Vec<CapturedPacket>, CaptureError> {
        // Would use pcap crate for real implementation
        Err(CaptureError::UnsupportedFormat)
    }
}

/// Capture replay engine
pub struct CaptureReplay {
    packets: Vec<CapturedPacket>,
    current_index: usize,
    start_time: Option<std::time::Instant>,
}

impl CaptureReplay {
    pub fn new(packets: Vec<CapturedPacket>) -> Self {
        Self {
            packets,
            current_index: 0,
            start_time: None,
        }
    }

    /// Get next inbound packet (sender -> receiver)
    pub fn next_inbound(&mut self) -> Option<&CapturedPacket> {
        while self.current_index < self.packets.len() {
            let packet = &self.packets[self.current_index];
            self.current_index += 1;
            if packet.inbound {
                return Some(packet);
            }
        }
        None
    }

    /// Get next packet with timing
    pub async fn next_timed(&mut self) -> Option<&CapturedPacket> {
        if self.current_index >= self.packets.len() {
            return None;
        }

        let packet = &self.packets[self.current_index];

        // Wait for correct time
        if let Some(start) = self.start_time {
            let target = Duration::from_micros(packet.timestamp_us);
            let elapsed = start.elapsed();
            if target > elapsed {
                tokio::time::sleep(target - elapsed).await;
            }
        } else {
            self.start_time = Some(std::time::Instant::now());
        }

        self.current_index += 1;
        Some(packet)
    }

    /// Reset replay
    pub fn reset(&mut self) {
        self.current_index = 0;
        self.start_time = None;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid capture format")]
    InvalidFormat,

    #[error("Invalid hex data")]
    InvalidHex,

    #[error("Unsupported capture format")]
    UnsupportedFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_replay() {
        let packets = vec![
            CapturedPacket {
                timestamp_us: 0,
                inbound: true,
                protocol: CaptureProtocol::Tcp,
                data: vec![1, 2, 3],
            },
            CapturedPacket {
                timestamp_us: 1000,
                inbound: false,
                protocol: CaptureProtocol::Tcp,
                data: vec![4, 5, 6],
            },
            CapturedPacket {
                timestamp_us: 2000,
                inbound: true,
                protocol: CaptureProtocol::Tcp,
                data: vec![7, 8, 9],
            },
        ];

        let mut replay = CaptureReplay::new(packets);

        // Should get inbound packets only
        let p1 = replay.next_inbound().unwrap();
        assert_eq!(p1.data, vec![1, 2, 3]);

        let p2 = replay.next_inbound().unwrap();
        assert_eq!(p2.data, vec![7, 8, 9]);

        assert!(replay.next_inbound().is_none());
    }
}
```

---

### 61.3 Test Utilities

**File:** `src/testing/test_utils.rs`

```rust
//! Test Utilities for AirPlay 2 Receiver

use crate::receiver::ap2::{Ap2Config, AirPlay2Receiver};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Create a test receiver with random port
pub async fn create_test_receiver() -> (AirPlay2Receiver, u16) {
    let port = portpicker::pick_unused_port().expect("No free ports");

    let config = Ap2Config::new("Test Receiver")
        .with_port(port);

    let receiver = AirPlay2Receiver::new(config).unwrap();
    (receiver, port)
}

/// Generate test audio data (sine wave)
pub fn generate_test_audio(
    frequency: f32,
    sample_rate: u32,
    duration_ms: u32,
    channels: u8,
) -> Vec<i16> {
    let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;
    let mut samples = Vec::with_capacity(num_samples * channels as usize);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let value = (2.0 * std::f32::consts::PI * frequency * t).sin();
        let sample = (value * 16000.0) as i16;

        for _ in 0..channels {
            samples.push(sample);
        }
    }

    samples
}

/// Compare audio samples with tolerance
pub fn samples_match(a: &[i16], b: &[i16], tolerance: i16) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for (sa, sb) in a.iter().zip(b.iter()) {
        if (sa - sb).abs() > tolerance {
            return false;
        }
    }

    true
}

/// Wait for condition with timeout
pub async fn wait_for<F>(
    condition: F,
    timeout_ms: u64,
    check_interval_ms: u64,
) -> bool
where
    F: Fn() -> bool,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let interval = std::time::Duration::from_millis(check_interval_ms);

    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(interval).await;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_generation() {
        let samples = generate_test_audio(440.0, 44100, 100, 2);

        // 100ms at 44100Hz stereo = 4410 * 2 samples
        assert_eq!(samples.len(), 8820);
    }

    #[test]
    fn test_samples_match() {
        let a = vec![100, 200, 300];
        let b = vec![101, 199, 302];

        assert!(samples_match(&a, &b, 5));
        assert!(!samples_match(&a, &b, 1));
    }
}
```

---

## Acceptance Criteria

- [ ] Mock sender connects and performs session
- [ ] Packet capture loading works
- [ ] Capture replay maintains timing
- [ ] Test utilities simplify test writing
- [ ] All unit tests pass
- [ ] Can run tests without real devices

---

## References

- [Section 19: Mock Server](./complete/19-mock-server.md)
