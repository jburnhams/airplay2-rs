//! Multi-phase SETUP handler for `AirPlay` 2
//!
//! Handles the two-phase SETUP process that configures event, timing,
//! and audio channels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tracing::{error, info, warn};

use super::body_handler::{encode_bplist_body, parse_bplist_body};
use super::request_handler::{Ap2Event, Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::session_state::Ap2SessionState;
use super::stream::{
    AudioStreamFormat, EncryptionType, StreamType, TimingPeerInfo, TimingProtocol,
};
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{RtspRequest, StatusCode};

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
    pub sender_address: Option<std::net::SocketAddr>,
}

impl SetupRequest {
    /// Parse SETUP request from binary plist body
    ///
    /// # Errors
    ///
    /// Returns error if plist parsing fails or structure is invalid
    pub fn parse(body: &[u8]) -> Result<Self, SetupParseError> {
        let plist =
            parse_bplist_body(body).map_err(|e| SetupParseError::InvalidPlist(e.to_string()))?;

        Self::from_plist(&plist)
    }

    /// Parse from already-decoded plist
    ///
    /// # Errors
    ///
    /// Returns error if plist structure is invalid
    pub fn from_plist(plist: &PlistValue) -> Result<Self, SetupParseError> {
        let PlistValue::Dictionary(dict) = plist else {
            return Err(SetupParseError::InvalidStructure(
                "Expected dictionary".into(),
            ));
        };

        // Parse streams array
        let streams = Self::parse_streams(dict)?;

        // Parse timing protocol
        let timing_protocol = dict
            .get("timingProtocol")
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
        let group_uuid = dict.get("groupUUID").and_then(|v| {
            if let PlistValue::String(s) = v {
                Some(s.clone())
            } else {
                None
            }
        });

        // Parse encryption type
        let encryption_type = dict
            .get("et")
            .and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    Some(match i {
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
        let shared_key = dict.get("shk").and_then(|v| {
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

    fn parse_streams(
        dict: &HashMap<String, PlistValue>,
    ) -> Result<Vec<StreamRequest>, SetupParseError> {
        let streams_value = dict
            .get("streams")
            .ok_or(SetupParseError::MissingField("streams"))?;

        let PlistValue::Array(streams_array) = streams_value else {
            return Err(SetupParseError::InvalidStructure(
                "streams must be array".into(),
            ));
        };

        let mut streams = Vec::new();

        for stream_plist in streams_array {
            let PlistValue::Dictionary(stream_dict) = stream_plist else {
                continue;
            };

            let stream_type = stream_dict
                .get("type")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        u32::try_from(*i).ok().map(StreamType::from)
                    } else {
                        None
                    }
                })
                .unwrap_or(StreamType::Unknown(0));

            let control_port = stream_dict.get("controlPort").and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    u16::try_from(*i).ok()
                } else {
                    None
                }
            });

            let data_port = stream_dict.get("dataPort").and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    u16::try_from(*i).ok()
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
                sender_address: None, // Filled in by session manager
            });
        }

        Ok(streams)
    }

    fn parse_audio_format(dict: &HashMap<String, PlistValue>) -> Option<AudioStreamFormat> {
        let codec = dict.get("ct").and_then(|v| {
            if let PlistValue::Integer(i) = v {
                u32::try_from(*i).ok()
            } else {
                None
            }
        })?;

        Some(AudioStreamFormat {
            codec,
            sample_rate: dict
                .get("sr")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        u32::try_from(*i).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(44100),
            channels: dict
                .get("ch")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        u8::try_from(*i).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(2),
            bits_per_sample: dict
                .get("ss")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        u8::try_from(*i).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(16),
            frames_per_packet: dict
                .get("fp")
                .and_then(|v| {
                    if let PlistValue::Integer(i) = v {
                        u32::try_from(*i).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(352),
            compression_type: dict.get("compressionType").and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    u32::try_from(*i).ok()
                } else {
                    None
                }
            }),
            spf: dict.get("spf").and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    u32::try_from(*i).ok()
                } else {
                    None
                }
            }),
        })
    }

    fn parse_timing_peer_info(dict: &HashMap<String, PlistValue>) -> Option<TimingPeerInfo> {
        let peer_info = dict.get("timingPeerInfo")?;
        let PlistValue::Dictionary(info_dict) = peer_info else {
            return None;
        };

        let peer_id = info_dict
            .get("ID")
            .and_then(|v| {
                if let PlistValue::Integer(i) = v {
                    u64::try_from(*i).ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let addresses = info_dict
            .get("Addresses")
            .and_then(|v| {
                if let PlistValue::Array(arr) = v {
                    Some(
                        arr.iter()
                            .filter_map(|a| {
                                if let PlistValue::String(s) = a {
                                    s.parse().ok()
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Some(TimingPeerInfo { peer_id, addresses })
    }

    /// Check if this is phase 1 (event/timing)
    #[must_use]
    pub fn is_phase1(&self) -> bool {
        self.streams
            .iter()
            .any(|s| matches!(s.stream_type, StreamType::Event | StreamType::Timing))
    }

    /// Check if this is phase 2 (audio)
    #[must_use]
    pub fn is_phase2(&self) -> bool {
        self.streams
            .iter()
            .any(|s| matches!(s.stream_type, StreamType::Audio | StreamType::BufferedAudio))
    }
}

/// Errors occurring during SETUP parsing
#[derive(Debug, thiserror::Error)]
pub enum SetupParseError {
    /// Invalid plist data
    #[error("Invalid plist: {0}")]
    InvalidPlist(String),

    /// Invalid plist structure (missing expected fields or wrong types)
    #[error("Invalid structure: {0}")]
    InvalidStructure(String),

    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

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
    /// Stream type
    pub stream_type: StreamType,
    /// Data port (if allocated)
    pub data_port: Option<u16>,
    /// Control port (if allocated)
    pub control_port: Option<u16>,
    /// Stream ID (assigned by server)
    pub stream_id: u64,
}

impl SetupResponse {
    /// Create phase 1 response (event + timing)
    #[must_use]
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
    #[must_use]
    pub fn phase2(data_port: u16, control_port: u16, audio_latency: u32) -> Self {
        Self {
            event_port: None,
            timing_port: None,
            data_port: Some(data_port),
            control_port: Some(control_port),
            audio_latency,
            timing_peer_info: None,
            streams: vec![StreamResponse {
                stream_type: StreamType::Audio,
                data_port: Some(data_port),
                control_port: Some(control_port),
                stream_id: 3,
            }],
        }
    }

    /// Convert to binary plist
    #[must_use]
    pub fn to_plist(&self) -> PlistValue {
        let mut dict: HashMap<String, PlistValue> = HashMap::new();

        // Event port
        if let Some(port) = self.event_port {
            dict.insert(
                "eventPort".to_string(),
                PlistValue::Integer(i64::from(port)),
            );
        }

        // Timing port
        if let Some(port) = self.timing_port {
            dict.insert(
                "timingPort".to_string(),
                PlistValue::Integer(i64::from(port)),
            );
        }

        // Audio ports
        if let Some(port) = self.data_port {
            dict.insert("dataPort".to_string(), PlistValue::Integer(i64::from(port)));
        }
        if let Some(port) = self.control_port {
            dict.insert(
                "controlPort".to_string(),
                PlistValue::Integer(i64::from(port)),
            );
        }

        // Audio latency
        if self.audio_latency > 0 {
            dict.insert(
                "audioLatency".to_string(),
                PlistValue::Integer(i64::from(self.audio_latency)),
            );
        }

        // Streams array
        let streams: Vec<PlistValue> = self
            .streams
            .iter()
            .map(|s| {
                let mut stream_dict: HashMap<String, PlistValue> = HashMap::new();
                stream_dict.insert(
                    "type".to_string(),
                    PlistValue::Integer(i64::from(s.stream_type)),
                );
                #[allow(clippy::cast_possible_wrap)]
                stream_dict.insert(
                    "streamID".to_string(),
                    PlistValue::Integer(s.stream_id as i64),
                );

                if let Some(port) = s.data_port {
                    stream_dict
                        .insert("dataPort".to_string(), PlistValue::Integer(i64::from(port)));
                }
                if let Some(port) = s.control_port {
                    stream_dict.insert(
                        "controlPort".to_string(),
                        PlistValue::Integer(i64::from(port)),
                    );
                }

                PlistValue::Dictionary(stream_dict)
            })
            .collect();

        dict.insert("streams".to_string(), PlistValue::Array(streams));

        PlistValue::Dictionary(dict)
    }
}

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
    /// Create a new port allocator with specified range
    #[must_use]
    pub fn new(range_start: u16, range_end: u16) -> Self {
        Self {
            next_port: range_start,
            allocated: Vec::new(),
            range_start,
            range_end,
        }
    }

    /// Allocate a port
    ///
    /// # Errors
    ///
    /// Returns error if no ports are available in the configured range
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
    #[allow(dead_code)] // May be unused in some contexts
    pub fn release_all(&mut self) {
        self.allocated.clear();
    }
}

/// Error allocating a port
#[derive(Debug, Copy, Clone, thiserror::Error)]
pub enum PortAllocationError {
    /// No ports available in the configured range
    #[error("No ports available in range")]
    NoPortsAvailable,
}

/// SETUP handler
pub struct SetupHandler {
    /// Port allocator
    port_allocator: Arc<Mutex<PortAllocator>>,
    /// Current setup phase
    pub(crate) current_phase: Arc<Mutex<SetupPhase>>,
    /// Audio latency in samples
    audio_latency_samples: u32,
    /// Allocated ports for current session
    session_ports: Arc<Mutex<SessionPorts>>,
}

/// Setup phases
#[derive(Debug, Clone, Copy, Default)]
pub enum SetupPhase {
    /// Initial state
    #[default]
    None,
    /// Phase 1 complete (event/timing)
    #[allow(dead_code)] // Used in tests
    Phase1Complete,
    /// Phase 2 complete (audio)
    #[allow(dead_code)] // Used in tests
    Phase2Complete,
}

/// Ports allocated for a session
#[derive(Debug, Clone, Default)]
pub struct SessionPorts {
    /// Event port
    pub event_port: Option<u16>,
    /// Timing port
    pub timing_port: Option<u16>,
    /// Audio data port
    pub audio_data_port: Option<u16>,
    /// Audio control port
    pub audio_control_port: Option<u16>,
}

impl SetupHandler {
    /// Create a new SETUP handler
    #[must_use]
    pub fn new(port_range_start: u16, port_range_end: u16, audio_latency_samples: u32) -> Self {
        Self {
            port_allocator: Arc::new(Mutex::new(PortAllocator::new(
                port_range_start,
                port_range_end,
            ))),
            current_phase: Arc::new(Mutex::new(SetupPhase::None)),
            audio_latency_samples,
            session_ports: Arc::new(Mutex::new(SessionPorts::default())),
        }
    }

    /// Handle SETUP request
    pub fn handle(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext<'_>,
    ) -> Ap2HandleResult {
        // Parse SETUP request
        let setup_request = match SetupRequest::parse(&request.body) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse SETUP request: {e}");
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Parse error: {e}")),
                };
            }
        };

        // Determine phase and handle
        if setup_request.is_phase1() {
            self.handle_phase1(setup_request, cseq)
        } else if setup_request.is_phase2() {
            self.handle_phase2(setup_request, cseq)
        } else {
            warn!("SETUP request with unknown stream types");
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

    fn handle_phase1(&self, request: SetupRequest, cseq: u32) -> Ap2HandleResult {
        let mut allocator = self.port_allocator.lock().unwrap();
        let mut session_ports = self.session_ports.lock().unwrap();

        // Allocate event and timing ports
        let event_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => return Self::allocation_error(cseq, e),
        };

        let timing_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => {
                allocator.release(event_port);
                return Self::allocation_error(cseq, e);
            }
        };

        // Store allocated ports
        session_ports.event_port = Some(event_port);
        session_ports.timing_port = Some(timing_port);

        // Update phase
        *self.current_phase.lock().unwrap() = SetupPhase::Phase1Complete;

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
                    error: Some(format!("Encode error: {e}")),
                };
            }
        };

        info!(
            "SETUP phase 1 complete: event={}, timing={}",
            event_port, timing_port
        );

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
                timing_peer_info: request.timing_peer_info,
                timing_protocol: request.timing_protocol,
            }),
            error: None,
        }
    }

    fn handle_phase2(&self, request: SetupRequest, cseq: u32) -> Ap2HandleResult {
        let mut allocator = self.port_allocator.lock().unwrap();
        let mut session_ports = self.session_ports.lock().unwrap();

        // Allocate audio ports
        let data_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => return Self::allocation_error(cseq, e),
        };

        let control_port = match allocator.allocate() {
            Ok(p) => p,
            Err(e) => {
                allocator.release(data_port);
                return Self::allocation_error(cseq, e);
            }
        };

        // Store allocated ports
        session_ports.audio_data_port = Some(data_port);
        session_ports.audio_control_port = Some(control_port);

        // Update phase
        *self.current_phase.lock().unwrap() = SetupPhase::Phase2Complete;

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
                    error: Some(format!("Encode error: {e}")),
                };
            }
        };

        info!(
            "SETUP phase 2 complete: data={}, control={}, latency={}",
            data_port, control_port, self.audio_latency_samples
        );

        // Extract audio stream info
        // We assume the first audio stream is the relevant one for configuration
        let audio_stream = request
            .streams
            .iter()
            .find(|s| matches!(s.stream_type, StreamType::Audio | StreamType::BufferedAudio));

        let audio_format = audio_stream.and_then(|s| s.audio_format.clone());

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
                audio_format,
                encryption_type: request.encryption_type,
                shared_key: request.shared_key,
            }),
            error: None,
        }
    }

    fn allocation_error(cseq: u32, error: PortAllocationError) -> Ap2HandleResult {
        Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode(453)) // Not Enough Bandwidth
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(error.to_string()),
        }
    }

    /// Release all ports for session cleanup
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn cleanup(&self) {
        let mut allocator = self.port_allocator.lock().unwrap();
        let session_ports = self.session_ports.lock().unwrap();

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

        *self.current_phase.lock().unwrap() = SetupPhase::None;
    }
}
