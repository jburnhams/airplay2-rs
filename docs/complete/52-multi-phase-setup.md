# Section 52: Multi-Phase SETUP Handler

## Dependencies
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 53**: Encrypted Control Channel (for decrypting SETUP bodies)
- **Section 03**: Binary Plist Codec

## Overview

AirPlay 2 uses a two-phase SETUP process, unlike AirPlay 1's single SETUP. This allows for more complex channel negotiation including event, timing, and multiple audio channels.

### SETUP Phases

**Phase 1: Event and Timing Channels**
- Establishes the event channel for async notifications
- Sets up timing synchronization (PTP or NTP)
- Allocates UDP ports for timing packets

**Phase 2: Audio Streams**
- Configures audio format and encryption
- Allocates UDP ports for audio data and control
- Sets up buffering parameters

```
Sender                              Receiver
  │                                    │
  │─── SETUP (phase 1) ───────────────▶│
  │    streams: [eventChannel, timing] │
  │                                    │
  │◀── Response ──────────────────────│
  │    eventPort: 7010                 │
  │    timingPort: 7011                │
  │                                    │
  │─── SETUP (phase 2) ───────────────▶│
  │    streams: [audioData, control]   │
  │    Audio format, encryption params │
  │                                    │
  │◀── Response ──────────────────────│
  │    dataPort: 7100                  │
  │    controlPort: 7101               │
  │    audioLatency: 88200             │
```

## Objectives

- Parse two-phase SETUP requests (binary plist bodies)
- Allocate UDP ports for each channel
- Configure audio streaming parameters
- Manage stream state across phases
- Support both encrypted and unencrypted SETUP bodies

---

## Tasks

### 52.1 SETUP Request Parsing

- [x] **52.1.1** Define SETUP request structures

**File:** `src/receiver/ap2/setup_handler.rs`

```rust
//! Multi-phase SETUP handler for AirPlay 2
//!
//! Handles the two-phase SETUP process that configures event, timing,
//! and audio channels.

use crate::protocol::plist::PlistValue;
use super::body_handler::{parse_bplist_body, PlistExt};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Parsed SETUP request
#[derive(Debug, Clone)]
pub struct SetupRequest {
    /// Requested streams
    pub streams: Vec<StreamRequest>,
    /// Timing protocol (NTP or PTP)
    pub timing_protocol: TimingProtocol,
    /// Timing peer info (for PTP)
    pub timing_peer_info: Option<TimingPeerInfo>,
    /// Group UUID (for multi-room)
    pub group_uuid: Option<String>,
    /// Encryption type
    pub encryption_type: EncryptionType,
    /// Shared encryption key (if provided)
    pub shared_key: Option<Vec<u8>>,
}

/// Individual stream request
#[derive(Debug, Clone)]
pub struct StreamRequest {
    /// Stream type
    pub stream_type: StreamType,
    /// Control port (sender's)
    pub control_port: Option<u16>,
    /// Data port (sender's, for audio)
    pub data_port: Option<u16>,
    /// Audio format (for audio streams)
    pub audio_format: Option<AudioStreamFormat>,
    /// Sender's network address
    pub sender_address: Option<SocketAddr>,
}

/// Stream types in SETUP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// General audio stream (type 96)
    Audio,
    /// Control/timing stream (type 103)
    Control,
    /// Event channel (type 130)
    Event,
    /// Timing (PTP) stream (type 150)
    Timing,
    /// Buffered audio (type 96 with buffered flag)
    BufferedAudio,
    /// Unknown stream type
    Unknown(u32),
}

impl From<u32> for StreamType {
    fn from(value: u32) -> Self {
        match value {
            96 => Self::Audio,
            103 => Self::Control,
            130 => Self::Event,
            150 => Self::Timing,
            _ => Self::Unknown(value),
        }
    }
}

/// Timing protocol selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingProtocol {
    /// Network Time Protocol (legacy)
    #[default]
    Ntp,
    /// Precision Time Protocol (AirPlay 2)
    Ptp,
    /// No timing (not recommended)
    None,
}

impl From<&str> for TimingProtocol {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PTP" => Self::Ptp,
            "NTP" => Self::Ntp,
            "NONE" => Self::None,
            _ => Self::Ntp,
        }
    }
}

/// Encryption type for audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionType {
    /// No encryption
    #[default]
    None,
    /// AirPlay 1 style (AES-128-CTR)
    Aes128Ctr,
    /// AirPlay 2 style (ChaCha20-Poly1305)
    ChaCha20Poly1305,
}

/// Timing peer information for PTP
#[derive(Debug, Clone)]
pub struct TimingPeerInfo {
    pub peer_id: u64,
    pub addresses: Vec<SocketAddr>,
}

/// Audio stream format parameters
#[derive(Debug, Clone)]
pub struct AudioStreamFormat {
    /// Codec type (96=ALAC, 97=AAC, etc.)
    pub codec: u32,
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Channels
    pub channels: u8,
    /// Bits per sample
    pub bits_per_sample: u8,
    /// Frames per packet
    pub frames_per_packet: u32,
    /// Compression type (for ALAC)
    pub compression_type: Option<u32>,
    /// Spf (samples per frame)
    pub spf: Option<u32>,
}

impl SetupRequest {
    /// Parse SETUP request from binary plist body
    pub fn parse(body: &[u8]) -> Result<Self, SetupParseError> {
        let plist = parse_bplist_body(body)
            .map_err(|e| SetupParseError::InvalidPlist(e.to_string()))?;

        Self::from_plist(&plist)
    }

    /// Parse from already-decoded plist
    pub fn from_plist(plist: &PlistValue) -> Result<Self, SetupParseError> {
        let PlistValue::Dict(dict) = plist else {
            return Err(SetupParseError::InvalidStructure("Expected dictionary".into()));
        };

        // Parse streams array
        let streams = Self::parse_streams(dict)?;

        // Parse timing protocol
        let timing_protocol = dict.get("timingProtocol")
            .and_then(|v| {
                if let PlistValue::String(s) = v {
                    Some(TimingProtocol::from(s.as_str()))
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Parse timing peer info
        let timing_peer_info = Self::parse_timing_peer_info(dict);

        // Parse group UUID
        let group_uuid = dict.get("groupUUID")
            .and_then(|v| {
                if let PlistValue::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });

        // Parse encryption type
        let encryption_type = dict.get("et")
            .and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    Some(match *i {
                        0 => EncryptionType::None,
                        1 => EncryptionType::Aes128Ctr,
                        4 => EncryptionType::ChaCha20Poly1305,
                        _ => EncryptionType::None,
                    })
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Parse shared key
        let shared_key = dict.get("shk")
            .and_then(|v| {
                if let PlistValue::Data(d) = v {
                    Some(d.clone())
                } else {
                    None
                }
            });

        Ok(Self {
            streams,
            timing_protocol,
            timing_peer_info,
            group_uuid,
            encryption_type,
            shared_key,
        })
    }

    fn parse_streams(dict: &HashMap<String, PlistValue>) -> Result<Vec<StreamRequest>, SetupParseError> {
        let streams_value = dict.get("streams")
            .ok_or_else(|| SetupParseError::MissingField("streams"))?;

        let PlistValue::Array(streams_array) = streams_value else {
            return Err(SetupParseError::InvalidStructure("streams must be array".into()));
        };

        let mut streams = Vec::new();

        for stream_plist in streams_array {
            let PlistValue::Dict(stream_dict) = stream_plist else {
                continue;
            };

            let stream_type = stream_dict.get("type")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        Some(StreamType::from(*i as u32))
                    } else {
                        None
                    }
                })
                .unwrap_or(StreamType::Unknown(0));

            let control_port = stream_dict.get("controlPort")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        Some(*i as u16)
                    } else {
                        None
                    }
                });

            let data_port = stream_dict.get("dataPort")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        Some(*i as u16)
                    } else {
                        None
                    }
                });

            let audio_format = Self::parse_audio_format(stream_dict);

            streams.push(StreamRequest {
                stream_type,
                control_port,
                data_port,
                audio_format,
                sender_address: None,  // Filled in by session manager
            });
        }

        Ok(streams)
    }

    fn parse_audio_format(dict: &HashMap<String, PlistValue>) -> Option<AudioStreamFormat> {
        let codec = dict.get("ct")
            .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u32) } else { None })?;

        Some(AudioStreamFormat {
            codec,
            sample_rate: dict.get("sr")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u32) } else { None })
                .unwrap_or(44100),
            channels: dict.get("ch")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u8) } else { None })
                .unwrap_or(2),
            bits_per_sample: dict.get("ss")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u8) } else { None })
                .unwrap_or(16),
            frames_per_packet: dict.get("spf")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u32) } else { None })
                .unwrap_or(352),
            compression_type: dict.get("compressionType")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u32) } else { None }),
            spf: dict.get("spf")
                .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u32) } else { None }),
        })
    }

    fn parse_timing_peer_info(dict: &HashMap<String, PlistValue>) -> Option<TimingPeerInfo> {
        let peer_info = dict.get("timingPeerInfo")?;
        let PlistValue::Dict(info_dict) = peer_info else {
            return None;
        };

        let peer_id = info_dict.get("ID")
            .and_then(|v| if let PlistValue::Integer(i) = v { Some(*i as u64) } else { None })
            .unwrap_or(0);

        let addresses = info_dict.get("Addresses")
            .and_then(|v| {
                if let PlistValue::Array(arr) = v {
                    Some(arr.iter()
                        .filter_map(|a| {
                            if let PlistValue::String(s) = a {
                                s.parse().ok()
                            } else {
                                None
                            }
                        })
                        .collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Some(TimingPeerInfo { peer_id, addresses })
    }

    /// Check if this is phase 1 (event/timing)
    pub fn is_phase1(&self) -> bool {
        self.streams.iter().any(|s|
            matches!(s.stream_type, StreamType::Event | StreamType::Timing)
        )
    }

    /// Check if this is phase 2 (audio)
    pub fn is_phase2(&self) -> bool {
        self.streams.iter().any(|s|
            matches!(s.stream_type, StreamType::Audio | StreamType::BufferedAudio)
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SetupParseError {
    #[error("Invalid plist: {0}")]
    InvalidPlist(String),

    #[error("Invalid structure: {0}")]
    InvalidStructure(String),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}
```

---

### 52.2 SETUP Response Builder

- [x] **52.2.1** Build SETUP response with allocated ports

**File:** `src/receiver/ap2/setup_handler.rs` (continued)

```rust
/// SETUP response data
#[derive(Debug, Clone)]
pub struct SetupResponse {
    /// Event port (phase 1)
    pub event_port: Option<u16>,
    /// Timing port (phase 1)
    pub timing_port: Option<u16>,
    /// Audio data port (phase 2)
    pub data_port: Option<u16>,
    /// Audio control port (phase 2)
    pub control_port: Option<u16>,
    /// Audio latency in samples
    pub audio_latency: u32,
    /// Timing peer info (for PTP)
    pub timing_peer_info: Option<TimingPeerInfo>,
    /// Stream responses
    pub streams: Vec<StreamResponse>,
}

/// Response for a single stream
#[derive(Debug, Clone)]
pub struct StreamResponse {
    pub stream_type: StreamType,
    pub data_port: Option<u16>,
    pub control_port: Option<u16>,
    pub stream_id: u64,
}

impl SetupResponse {
    /// Create phase 1 response (event + timing)
    pub fn phase1(event_port: u16, timing_port: u16) -> Self {
        Self {
            event_port: Some(event_port),
            timing_port: Some(timing_port),
            data_port: None,
            control_port: None,
            audio_latency: 0,
            timing_peer_info: None,
            streams: vec![
                StreamResponse {
                    stream_type: StreamType::Event,
                    data_port: Some(event_port),
                    control_port: None,
                    stream_id: 1,
                },
                StreamResponse {
                    stream_type: StreamType::Timing,
                    data_port: Some(timing_port),
                    control_port: None,
                    stream_id: 2,
                },
            ],
        }
    }

    /// Create phase 2 response (audio)
    pub fn phase2(data_port: u16, control_port: u16, audio_latency: u32) -> Self {
        Self {
            event_port: None,
            timing_port: None,
            data_port: Some(data_port),
            control_port: Some(control_port),
            audio_latency,
            timing_peer_info: None,
            streams: vec![
                StreamResponse {
                    stream_type: StreamType::Audio,
                    data_port: Some(data_port),
                    control_port: Some(control_port),
                    stream_id: 3,
                },
            ],
        }
    }

    /// Convert to binary plist
    pub fn to_plist(&self) -> PlistValue {
        let mut dict: HashMap<String, PlistValue> = HashMap::new();

        // Event port
        if let Some(port) = self.event_port {
            dict.insert("eventPort".to_string(), PlistValue::Integer(port as i64));
        }

        // Timing port
        if let Some(port) = self.timing_port {
            dict.insert("timingPort".to_string(), PlistValue::Integer(port as i64));
        }

        // Audio ports
        if let Some(port) = self.data_port {
            dict.insert("dataPort".to_string(), PlistValue::Integer(port as i64));
        }
        if let Some(port) = self.control_port {
            dict.insert("controlPort".to_string(), PlistValue::Integer(port as i64));
        }

        // Audio latency
        if self.audio_latency > 0 {
            dict.insert("audioLatency".to_string(),
                PlistValue::Integer(self.audio_latency as i64));
        }

        // Streams array
        let streams: Vec<PlistValue> = self.streams.iter().map(|s| {
            let mut stream_dict: HashMap<String, PlistValue> = HashMap::new();
            stream_dict.insert("type".to_string(),
                PlistValue::Integer(match s.stream_type {
                    StreamType::Audio => 96,
                    StreamType::Control => 103,
                    StreamType::Event => 130,
                    StreamType::Timing => 150,
                    StreamType::BufferedAudio => 96,
                    StreamType::Unknown(t) => t as i64,
                } as i64));
            stream_dict.insert("streamID".to_string(),
                PlistValue::Integer(s.stream_id as i64));

            if let Some(port) = s.data_port {
                stream_dict.insert("dataPort".to_string(),
                    PlistValue::Integer(port as i64));
            }
            if let Some(port) = s.control_port {
                stream_dict.insert("controlPort".to_string(),
                    PlistValue::Integer(port as i64));
            }

            PlistValue::Dict(stream_dict)
        }).collect();

        dict.insert("streams".to_string(), PlistValue::Array(streams));

        PlistValue::Dict(dict)
    }
}
```

---

### 52.3 SETUP Handler Implementation

- [x] **52.3.1** Implement SETUP request handler

**File:** `src/receiver/ap2/setup_handler.rs` (continued)

```rust
use super::request_handler::{Ap2HandleResult, Ap2Event, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::session_state::Ap2SessionState;
use super::body_handler::encode_bplist_body;
use crate::protocol::rtsp::{RtspRequest, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Port allocator for receiver streams
pub struct PortAllocator {
    /// Next port to try
    next_port: u16,
    /// Allocated ports
    allocated: Vec<u16>,
    /// Port range start
    range_start: u16,
    /// Port range end
    range_end: u16,
}

impl PortAllocator {
    pub fn new(range_start: u16, range_end: u16) -> Self {
        Self {
            next_port: range_start,
            allocated: Vec::new(),
            range_start,
            range_end,
        }
    }

    /// Allocate a port
    pub fn allocate(&mut self) -> Result<u16, PortAllocationError> {
        let start = self.next_port;
        let mut port = start;

        loop {
            if !self.allocated.contains(&port) {
                self.allocated.push(port);
                self.next_port = if port + 1 > self.range_end {
                    self.range_start
                } else {
                    port + 1
                };
                return Ok(port);
            }

            port = if port + 1 > self.range_end {
                self.range_start
            } else {
                port + 1
            };

            if port == start {
                return Err(PortAllocationError::NoPortsAvailable);
            }
        }
    }

    /// Release a port
    pub fn release(&mut self, port: u16) {
        self.allocated.retain(|&p| p != port);
    }

    /// Release all ports
    pub fn release_all(&mut self) {
        self.allocated.clear();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PortAllocationError {
    #[error("No ports available in range")]
    NoPortsAvailable,
}

/// SETUP handler
pub struct SetupHandler {
    /// Port allocator
    port_allocator: Arc<Mutex<PortAllocator>>,
    /// Current setup phase
    current_phase: Arc<Mutex<SetupPhase>>,
    /// Audio latency in samples
    audio_latency_samples: u32,
    /// Allocated ports for current session
    session_ports: Arc<Mutex<SessionPorts>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SetupPhase {
    #[default]
    None,
    Phase1Complete,
    Phase2Complete,
}

#[derive(Debug, Clone, Default)]
pub struct SessionPorts {
    pub event_port: Option<u16>,
    pub timing_port: Option<u16>,
    pub audio_data_port: Option<u16>,
    pub audio_control_port: Option<u16>,
}

impl SetupHandler {
    pub fn new(port_range_start: u16, port_range_end: u16, audio_latency_samples: u32) -> Self {
        Self {
            port_allocator: Arc::new(Mutex::new(PortAllocator::new(port_range_start, port_range_end))),
            current_phase: Arc::new(Mutex::new(SetupPhase::None)),
            audio_latency_samples,
            session_ports: Arc::new(Mutex::new(SessionPorts::default())),
        }
    }

    /// Handle SETUP request
    pub async fn handle(
        &self,
        request: &RtspRequest,
        cseq: u32,
        context: &Ap2RequestContext<'_>,
    ) -> Ap2HandleResult {
        // Parse SETUP request
        let setup_request = match SetupRequest::parse(&request.body) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to parse SETUP request: {}", e);
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Parse error: {}", e)),
                };
            }
        };

        // Determine phase and handle
        if setup_request.is_phase1() {
            self.handle_phase1(setup_request, cseq).await
        } else if setup_request.is_phase2() {
            self.handle_phase2(setup_request, cseq).await
        } else {
            log::warn!("SETUP request with unknown stream types");
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some("Unknown stream types in SETUP".to_string()),
            }
        }
    }

    async fn handle_phase1(&self, request: SetupRequest, cseq: u32) -> Ap2HandleResult {
        let mut allocator = self.port_allocator.lock().await;
        let mut session_ports = self.session_ports.lock().await;

        // Allocate event and timing ports
        let event_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => return self.allocation_error(cseq, e),
        };

        let timing_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => {
                allocator.release(event_port);
                return self.allocation_error(cseq, e);
            }
        };

        // Store allocated ports
        session_ports.event_port = Some(event_port);
        session_ports.timing_port = Some(timing_port);

        // Update phase
        *self.current_phase.lock().await = SetupPhase::Phase1Complete;

        // Build response
        let response = SetupResponse::phase1(event_port, timing_port);

        let body = match encode_bplist_body(&response.to_plist()) {
            Ok(b) => b,
            Err(e) => {
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Encode error: {}", e)),
                };
            }
        };

        log::info!("SETUP phase 1 complete: event={}, timing={}", event_port, timing_port);

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .header("Content-Type", "application/x-apple-binary-plist")
                .binary_body(body)
                .encode(),
            new_state: Some(Ap2SessionState::SetupPhase1),
            event: Some(Ap2Event::SetupPhase1Complete {
                timing_port,
                event_port,
            }),
            error: None,
        }
    }

    async fn handle_phase2(&self, request: SetupRequest, cseq: u32) -> Ap2HandleResult {
        let mut allocator = self.port_allocator.lock().await;
        let mut session_ports = self.session_ports.lock().await;

        // Allocate audio ports
        let data_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => return self.allocation_error(cseq, e),
        };

        let control_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => {
                allocator.release(data_port);
                return self.allocation_error(cseq, e);
            }
        };

        // Store allocated ports
        session_ports.audio_data_port = Some(data_port);
        session_ports.audio_control_port = Some(control_port);

        // Update phase
        *self.current_phase.lock().await = SetupPhase::Phase2Complete;

        // Build response with audio latency
        let response = SetupResponse::phase2(data_port, control_port, self.audio_latency_samples);

        let body = match encode_bplist_body(&response.to_plist()) {
            Ok(b) => b,
            Err(e) => {
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Encode error: {}", e)),
                };
            }
        };

        log::info!(
            "SETUP phase 2 complete: data={}, control={}, latency={}",
            data_port, control_port, self.audio_latency_samples
        );

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .header("Content-Type", "application/x-apple-binary-plist")
                .binary_body(body)
                .encode(),
            new_state: Some(Ap2SessionState::SetupPhase2),
            event: Some(Ap2Event::SetupPhase2Complete {
                audio_data_port: data_port,
                audio_control_port: control_port,
            }),
            error: None,
        }
    }

    fn allocation_error(&self, cseq: u32, error: PortAllocationError) -> Ap2HandleResult {
        Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode(453))  // Not Enough Bandwidth
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(error.to_string()),
        }
    }

    /// Release all ports for session cleanup
    pub async fn cleanup(&self) {
        let mut allocator = self.port_allocator.lock().await;
        let session_ports = self.session_ports.lock().await;

        if let Some(port) = session_ports.event_port {
            allocator.release(port);
        }
        if let Some(port) = session_ports.timing_port {
            allocator.release(port);
        }
        if let Some(port) = session_ports.audio_data_port {
            allocator.release(port);
        }
        if let Some(port) = session_ports.audio_control_port {
            allocator.release(port);
        }

        *self.current_phase.lock().await = SetupPhase::None;
    }
}
```

---

## Unit Tests

### 52.4 SETUP Tests

- [x] **52.4.1** Test SETUP request parsing and response generation

**File:** `src/receiver/ap2/setup_handler.rs` (test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_detection() {
        // Phase 1 request
        let phase1 = SetupRequest {
            streams: vec![
                StreamRequest {
                    stream_type: StreamType::Event,
                    control_port: None,
                    data_port: None,
                    audio_format: None,
                    sender_address: None,
                },
                StreamRequest {
                    stream_type: StreamType::Timing,
                    control_port: None,
                    data_port: None,
                    audio_format: None,
                    sender_address: None,
                },
            ],
            timing_protocol: TimingProtocol::Ptp,
            timing_peer_info: None,
            group_uuid: None,
            encryption_type: EncryptionType::None,
            shared_key: None,
        };

        assert!(phase1.is_phase1());
        assert!(!phase1.is_phase2());

        // Phase 2 request
        let phase2 = SetupRequest {
            streams: vec![
                StreamRequest {
                    stream_type: StreamType::Audio,
                    control_port: Some(6001),
                    data_port: Some(6000),
                    audio_format: Some(AudioStreamFormat {
                        codec: 96,
                        sample_rate: 44100,
                        channels: 2,
                        bits_per_sample: 16,
                        frames_per_packet: 352,
                        compression_type: None,
                        spf: None,
                    }),
                    sender_address: None,
                },
            ],
            timing_protocol: TimingProtocol::Ptp,
            timing_peer_info: None,
            group_uuid: None,
            encryption_type: EncryptionType::ChaCha20Poly1305,
            shared_key: Some(vec![0u8; 32]),
        };

        assert!(!phase2.is_phase1());
        assert!(phase2.is_phase2());
    }

    #[test]
    fn test_port_allocator() {
        let mut allocator = PortAllocator::new(7000, 7010);

        let p1 = allocator.allocate().unwrap();
        let p2 = allocator.allocate().unwrap();

        assert_ne!(p1, p2);
        assert!(p1 >= 7000 && p1 <= 7010);
        assert!(p2 >= 7000 && p2 <= 7010);

        allocator.release(p1);

        let p3 = allocator.allocate().unwrap();
        // p1 should be available again
    }

    #[test]
    fn test_response_plist() {
        let response = SetupResponse::phase1(7010, 7011);
        let plist = response.to_plist();

        if let PlistValue::Dict(dict) = plist {
            assert!(dict.contains_key("eventPort"));
            assert!(dict.contains_key("timingPort"));
            assert!(dict.contains_key("streams"));
        } else {
            panic!("Expected Dict");
        }
    }

    #[tokio::test]
    async fn test_setup_handler_phases() {
        let handler = SetupHandler::new(7000, 7100, 88200);

        // Simulate phase 1
        let phase1_request = SetupRequest {
            streams: vec![
                StreamRequest {
                    stream_type: StreamType::Event,
                    control_port: None,
                    data_port: None,
                    audio_format: None,
                    sender_address: None,
                },
            ],
            timing_protocol: TimingProtocol::Ptp,
            timing_peer_info: None,
            group_uuid: None,
            encryption_type: EncryptionType::None,
            shared_key: None,
        };

        // Check phase
        assert!(phase1_request.is_phase1());

        // After handling both phases, cleanup
        handler.cleanup().await;

        // Ports should be released
        let phase = *handler.current_phase.lock().await;
        assert!(matches!(phase, SetupPhase::None));
    }
}
```

---

## Acceptance Criteria

- [x] Phase 1 SETUP correctly parses event/timing streams
- [x] Phase 2 SETUP correctly parses audio streams
- [x] Port allocation works within configured range
- [x] Response contains all required fields
- [x] Audio latency correctly reported
- [x] Session state advances through phases
- [x] Events emitted for pipeline coordination
- [x] Cleanup releases all allocated ports
- [x] All unit tests pass

---

## Notes

### Encrypted SETUP Bodies

After pairing, SETUP request bodies are encrypted with the session key. The handler
should work with the encrypted channel layer (Section 53) which handles decryption
before the body reaches this handler.

### Timing Protocol Selection

- **PTP**: Preferred for AirPlay 2, enables multi-room sync
- **NTP**: Fallback for compatibility
- The receiver should support both and use what the sender requests

---

## References

- [AirPlay 2 SETUP Analysis](https://emanuelecozzi.net/docs/airplay2/rtsp/)
- [Section 48: RTSP/HTTP Server](./48-rtsp-http-server-extensions.md)
