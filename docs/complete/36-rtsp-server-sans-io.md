# Section 36: RTSP Server (Sans-IO)

> **VERIFIED**: Checked against `src/protocol/rtsp/server_codec.rs` on 2025-01-30.
> Server-side RTSP codec fully implemented.

## Dependencies
- **Section 05**: RTSP Protocol (existing codec and types)
- **Section 34**: Receiver Overview (architectural context)
- **Section 02**: Core Types, Errors & Config

## Overview

This section implements the server-side RTSP handling for the AirPlay 1 receiver. The existing RTSP implementation (Section 05) provides client-side functionality; here we create a complementary server-side codec that:

1. **Parses incoming RTSP requests** from AirPlay senders
2. **Generates RTSP responses** with proper formatting
3. **Follows sans-IO principles** - no I/O in the codec itself
4. **Reuses existing types** - `RtspRequest`, `RtspResponse`, `Headers`, `Method`

The key insight is that RTSP requests and responses have symmetric structures, so we can reuse most existing types while adding server-specific parsing and response generation.

## Objectives

- Implement `RtspServerCodec` for server-side request parsing
- Create response builder for proper RTSP response formatting
- Handle all RAOP-specific RTSP methods
- Maintain CSeq tracking and session management
- Support binary and text body payloads
- Ensure complete test coverage with real-world RTSP captures

---

## Tasks

### 36.1 Request Parser

- [x] **36.1.1** Implement server-side RTSP request parser

**File:** `src/protocol/rtsp/server_codec.rs`

```rust
//! Server-side RTSP codec for parsing requests and generating responses
//!
//! This module complements the client-side codec by providing server-side
//! parsing. Both share the same request/response types but differ in what
//! they parse vs. generate.

use super::{RtspRequest, RtspResponse, Method, Headers, StatusCode};
use bytes::{Buf, BytesMut};
use std::str;

/// Errors during RTSP parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Incomplete data, need more bytes")]
    Incomplete,

    #[error("Invalid request line: {0}")]
    InvalidRequestLine(String),

    #[error("Invalid method: {0}")]
    InvalidMethod(String),

    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    #[error("Invalid Content-Length: {0}")]
    InvalidContentLength(String),

    #[error("Body too large: {size} > {max}")]
    BodyTooLarge { size: usize, max: usize },

    #[error("Invalid UTF-8 in headers")]
    InvalidUtf8,
}

/// Maximum allowed body size (16 MB should be plenty for any RTSP body)
const MAX_BODY_SIZE: usize = 16 * 1024 * 1024;

/// Maximum header section size (64 KB)
const MAX_HEADER_SIZE: usize = 64 * 1024;

/// Server-side RTSP codec
///
/// Parses incoming RTSP requests from a byte buffer and generates
/// properly formatted RTSP responses.
///
/// # Sans-IO Design
///
/// This codec performs no I/O. It operates on byte buffers:
/// - `feed()` adds bytes to the internal buffer
/// - `decode()` attempts to parse a complete request
/// - `encode_response()` generates response bytes
///
/// # Example
///
/// ```rust
/// use airplay2::protocol::rtsp::RtspServerCodec;
///
/// let mut codec = RtspServerCodec::new();
///
/// // Feed incoming bytes
/// codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
///
/// // Try to decode
/// if let Some(request) = codec.decode()? {
///     println!("Method: {:?}", request.method);
/// }
/// ```
pub struct RtspServerCodec {
    buffer: BytesMut,
}

impl RtspServerCodec {
    /// Create a new server codec
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Feed bytes into the internal buffer
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Get current buffer length
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Attempt to decode a complete RTSP request
    ///
    /// Returns:
    /// - `Ok(Some(request))` if a complete request was parsed
    /// - `Ok(None)` if more data is needed
    /// - `Err(e)` if parsing failed
    pub fn decode(&mut self) -> Result<Option<RtspRequest>, ParseError> {
        // Find header/body separator
        let header_end = match self.find_header_end() {
            Some(pos) => pos,
            None => {
                // Check for header overflow
                if self.buffer.len() > MAX_HEADER_SIZE {
                    return Err(ParseError::InvalidHeader("Headers too large".into()));
                }
                return Ok(None);  // Need more data
            }
        };

        // Parse headers (without consuming buffer yet)
        let header_bytes = &self.buffer[..header_end];
        let header_str = str::from_utf8(header_bytes)
            .map_err(|_| ParseError::InvalidUtf8)?;

        let (method, uri, headers) = self.parse_headers(header_str)?;

        // Determine body length
        let content_length = headers
            .get("Content-Length")
            .or_else(|| headers.get("content-length"))
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| ParseError::InvalidContentLength("Not a number".into()))?
            .unwrap_or(0);

        if content_length > MAX_BODY_SIZE {
            return Err(ParseError::BodyTooLarge {
                size: content_length,
                max: MAX_BODY_SIZE,
            });
        }

        // Total message size: headers + \r\n\r\n + body
        let total_size = header_end + 4 + content_length;

        if self.buffer.len() < total_size {
            return Ok(None);  // Need more data for body
        }

        // Now consume the buffer
        let _ = self.buffer.split_to(header_end + 4);  // Headers + separator
        let body = if content_length > 0 {
            self.buffer.split_to(content_length).to_vec()
        } else {
            Vec::new()
        };

        Ok(Some(RtspRequest {
            method,
            uri,
            headers,
            body,
        }))
    }

    /// Find the position of header/body separator (\r\n\r\n)
    fn find_header_end(&self) -> Option<usize> {
        let needle = b"\r\n\r\n";
        self.buffer
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// Parse request line and headers
    fn parse_headers(&self, header_str: &str) -> Result<(Method, String, Headers), ParseError> {
        let mut lines = header_str.lines();

        // Parse request line: "METHOD uri RTSP/1.0"
        let request_line = lines.next()
            .ok_or_else(|| ParseError::InvalidRequestLine("Empty request".into()))?;

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(ParseError::InvalidRequestLine(request_line.to_string()));
        }

        let method = Method::from_str(parts[0])
            .ok_or_else(|| ParseError::InvalidMethod(parts[0].to_string()))?;
        let uri = parts[1].to_string();

        // Validate protocol version
        if !parts[2].starts_with("RTSP/") {
            return Err(ParseError::InvalidRequestLine(
                format!("Invalid protocol: {}", parts[2])
            ));
        }

        // Parse headers
        let mut headers = Headers::new();
        for line in lines {
            if line.is_empty() {
                break;
            }

            if let Some(pos) = line.find(':') {
                let name = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                headers.insert(name, value);
            } else {
                return Err(ParseError::InvalidHeader(line.to_string()));
            }
        }

        Ok((method, uri, headers))
    }
}

impl Default for RtspServerCodec {
    fn default() -> Self {
        Self::new()
    }
}
```

---

### 36.2 Response Builder

- [ ] **36.2.1** Implement RTSP response encoder

**File:** `src/protocol/rtsp/server_codec.rs` (continued)

```rust
/// Builder for RTSP responses
///
/// Provides a fluent interface for constructing RTSP responses with
/// proper formatting for the wire protocol.
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status: StatusCode,
    headers: Headers,
    body: Option<Vec<u8>>,
}

impl ResponseBuilder {
    /// Create a new response builder with the given status
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: None,
        }
    }

    /// Create an OK (200) response
    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    /// Create an error response
    pub fn error(status: StatusCode) -> Self {
        Self::new(status)
    }

    /// Set the CSeq header (required - should match request)
    pub fn cseq(mut self, cseq: u32) -> Self {
        self.headers.insert("CSeq".to_string(), cseq.to_string());
        self
    }

    /// Set the Session header
    pub fn session(mut self, session_id: &str) -> Self {
        self.headers.insert("Session".to_string(), session_id.to_string());
        self
    }

    /// Add a custom header
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Set a text body (will set Content-Type to text/parameters)
    pub fn text_body(mut self, body: &str) -> Self {
        self.body = Some(body.as_bytes().to_vec());
        self.headers.insert(
            "Content-Type".to_string(),
            "text/parameters".to_string()
        );
        self
    }

    /// Set a binary body
    pub fn binary_body(mut self, body: Vec<u8>, content_type: &str) -> Self {
        self.body = Some(body);
        self.headers.insert("Content-Type".to_string(), content_type.to_string());
        self
    }

    /// Set SDP body (for ANNOUNCE responses if needed)
    pub fn sdp_body(mut self, sdp: &str) -> Self {
        self.body = Some(sdp.as_bytes().to_vec());
        self.headers.insert(
            "Content-Type".to_string(),
            "application/sdp".to_string()
        );
        self
    }

    /// Set the Audio-Latency header (used in RECORD response)
    pub fn audio_latency(mut self, samples: u32) -> Self {
        self.headers.insert("Audio-Latency".to_string(), samples.to_string());
        self
    }

    /// Build into an RtspResponse
    pub fn build(mut self) -> RtspResponse {
        // Add Content-Length if body present
        if let Some(ref body) = self.body {
            self.headers.insert(
                "Content-Length".to_string(),
                body.len().to_string()
            );
        }

        RtspResponse {
            status: self.status,
            headers: self.headers,
            body: self.body.unwrap_or_default(),
        }
    }

    /// Encode directly to bytes
    pub fn encode(self) -> Vec<u8> {
        let response = self.build();
        encode_response(&response)
    }
}

/// Encode an RTSP response to bytes
pub fn encode_response(response: &RtspResponse) -> Vec<u8> {
    let mut output = Vec::with_capacity(256 + response.body.len());

    // Status line
    let reason = status_reason(response.status);
    output.extend_from_slice(
        format!("RTSP/1.0 {} {}\r\n", response.status.0, reason).as_bytes()
    );

    // Headers
    for (name, value) in response.headers.iter() {
        output.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
    }

    // Separator
    output.extend_from_slice(b"\r\n");

    // Body
    if !response.body.is_empty() {
        output.extend_from_slice(&response.body);
    }

    output
}

/// Get reason phrase for status code
fn status_reason(status: StatusCode) -> &'static str {
    match status.0 {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        451 => "Parameter Not Understood",
        453 => "Not Enough Bandwidth",
        454 => "Session Not Found",
        455 => "Method Not Valid in This State",
        457 => "Invalid Range",
        459 => "Aggregate Operation Not Allowed",
        460 => "Only Aggregate Operation Allowed",
        461 => "Unsupported Transport",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Common status codes for RAOP
impl StatusCode {
    pub const OK: StatusCode = StatusCode(200);
    pub const BAD_REQUEST: StatusCode = StatusCode(400);
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    pub const FORBIDDEN: StatusCode = StatusCode(403);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    pub const NOT_ACCEPTABLE: StatusCode = StatusCode(406);
    pub const SESSION_NOT_FOUND: StatusCode = StatusCode(454);
    pub const METHOD_NOT_VALID: StatusCode = StatusCode(455);
    pub const INTERNAL_ERROR: StatusCode = StatusCode(500);
    pub const NOT_IMPLEMENTED: StatusCode = StatusCode(501);
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);
}
```

---

### 36.3 Transport Header Parser

- [ ] **36.3.1** Implement Transport header parsing for SETUP requests

**File:** `src/protocol/rtsp/transport.rs`

```rust
//! RTSP Transport header parsing
//!
//! The Transport header in SETUP requests specifies how audio will be delivered.
//! Format: `RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002`

use std::collections::HashMap;

/// Parsed Transport header
#[derive(Debug, Clone, PartialEq)]
pub struct TransportHeader {
    /// Protocol (always "RTP/AVP" for RAOP)
    pub protocol: String,
    /// Lower protocol (UDP or TCP)
    pub lower_transport: LowerTransport,
    /// Unicast or multicast
    pub cast: CastMode,
    /// Mode (usually "record" for RAOP)
    pub mode: Option<String>,
    /// Client's control port
    pub control_port: Option<u16>,
    /// Client's timing port
    pub timing_port: Option<u16>,
    /// Interleaved channel (for TCP transport)
    pub interleaved: Option<(u8, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerTransport {
    Udp,
    Tcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastMode {
    Unicast,
    Multicast,
}

impl TransportHeader {
    /// Parse a Transport header value
    pub fn parse(value: &str) -> Result<Self, TransportParseError> {
        let mut parts = value.split(';');

        // First part: protocol specification
        let proto_spec = parts.next()
            .ok_or(TransportParseError::MissingProtocol)?;

        let (protocol, lower_transport) = Self::parse_protocol(proto_spec)?;

        let mut transport = TransportHeader {
            protocol,
            lower_transport,
            cast: CastMode::Unicast,  // Default
            mode: None,
            control_port: None,
            timing_port: None,
            interleaved: None,
        };

        // Parse remaining parameters
        for part in parts {
            let part = part.trim();

            if part == "unicast" {
                transport.cast = CastMode::Unicast;
            } else if part == "multicast" {
                transport.cast = CastMode::Multicast;
            } else if let Some(value) = part.strip_prefix("mode=") {
                transport.mode = Some(value.to_string());
            } else if let Some(value) = part.strip_prefix("control_port=") {
                transport.control_port = Some(
                    value.parse().map_err(|_| TransportParseError::InvalidPort)?
                );
            } else if let Some(value) = part.strip_prefix("timing_port=") {
                transport.timing_port = Some(
                    value.parse().map_err(|_| TransportParseError::InvalidPort)?
                );
            } else if let Some(value) = part.strip_prefix("interleaved=") {
                transport.interleaved = Some(Self::parse_interleaved(value)?);
            }
            // Ignore unknown parameters
        }

        Ok(transport)
    }

    fn parse_protocol(spec: &str) -> Result<(String, LowerTransport), TransportParseError> {
        let parts: Vec<&str> = spec.split('/').collect();

        match parts.as_slice() {
            ["RTP", "AVP"] => Ok(("RTP/AVP".to_string(), LowerTransport::Udp)),
            ["RTP", "AVP", "UDP"] => Ok(("RTP/AVP".to_string(), LowerTransport::Udp)),
            ["RTP", "AVP", "TCP"] => Ok(("RTP/AVP".to_string(), LowerTransport::Tcp)),
            _ => Err(TransportParseError::UnsupportedProtocol(spec.to_string())),
        }
    }

    fn parse_interleaved(value: &str) -> Result<(u8, u8), TransportParseError> {
        let parts: Vec<&str> = value.split('-').collect();
        match parts.as_slice() {
            [start, end] => {
                let start: u8 = start.parse().map_err(|_| TransportParseError::InvalidPort)?;
                let end: u8 = end.parse().map_err(|_| TransportParseError::InvalidPort)?;
                Ok((start, end))
            }
            _ => Err(TransportParseError::InvalidInterleaved),
        }
    }

    /// Generate Transport header for response
    pub fn to_response_header(
        &self,
        server_port: u16,
        control_port: u16,
        timing_port: u16,
    ) -> String {
        let mut parts = vec![
            format!("{}/{}", self.protocol,
                match self.lower_transport {
                    LowerTransport::Udp => "UDP",
                    LowerTransport::Tcp => "TCP",
                }
            ),
            self.cast.to_string(),
        ];

        if let Some(ref mode) = self.mode {
            parts.push(format!("mode={}", mode));
        }

        parts.push(format!("server_port={}", server_port));
        parts.push(format!("control_port={}", control_port));
        parts.push(format!("timing_port={}", timing_port));

        parts.join(";")
    }
}

impl std::fmt::Display for CastMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CastMode::Unicast => write!(f, "unicast"),
            CastMode::Multicast => write!(f, "multicast"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportParseError {
    #[error("Missing protocol specification")]
    MissingProtocol,

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Invalid port number")]
    InvalidPort,

    #[error("Invalid interleaved channel specification")]
    InvalidInterleaved,
}
```

---

### 36.4 Request Handlers

- [ ] **36.4.1** Implement RTSP method handler infrastructure

**File:** `src/receiver/rtsp_handler.rs`

```rust
//! RTSP request handlers for the receiver
//!
//! This module provides the logic for handling each RTSP method.
//! Handlers are pure functions that take a request and session state,
//! returning a response. No I/O is performed.

use crate::protocol::rtsp::{
    RtspRequest, Method, StatusCode,
    server_codec::ResponseBuilder,
    transport::TransportHeader,
};
use crate::receiver::session::{ReceiverSession, SessionState};

/// Result of handling an RTSP request
#[derive(Debug)]
pub struct HandleResult {
    /// Response to send back
    pub response: Vec<u8>,
    /// New session state (if changed)
    pub new_state: Option<SessionState>,
    /// Allocated ports (for SETUP)
    pub allocated_ports: Option<AllocatedPorts>,
    /// Should start streaming (for RECORD)
    pub start_streaming: bool,
    /// Should stop streaming (for TEARDOWN)
    pub stop_streaming: bool,
}

/// Ports allocated during SETUP
#[derive(Debug, Clone, Copy)]
pub struct AllocatedPorts {
    pub audio_port: u16,
    pub control_port: u16,
    pub timing_port: u16,
}

/// Handle an incoming RTSP request
pub fn handle_request(
    request: &RtspRequest,
    session: &ReceiverSession,
) -> HandleResult {
    let cseq = request.headers.cseq().unwrap_or(0);

    match request.method {
        Method::Options => handle_options(cseq),
        Method::Announce => handle_announce(request, cseq, session),
        Method::Setup => handle_setup(request, cseq, session),
        Method::Record => handle_record(request, cseq, session),
        Method::Pause => handle_pause(cseq, session),
        Method::Flush => handle_flush(request, cseq),
        Method::Teardown => handle_teardown(cseq, session),
        Method::GetParameter => handle_get_parameter(request, cseq, session),
        Method::SetParameter => handle_set_parameter(request, cseq, session),
        Method::Post => handle_post(request, cseq, session),
        _ => handle_unknown(cseq),
    }
}

/// Handle OPTIONS request
fn handle_options(cseq: u32) -> HandleResult {
    let methods = [
        "ANNOUNCE", "SETUP", "RECORD", "PAUSE", "FLUSH",
        "TEARDOWN", "OPTIONS", "GET_PARAMETER", "SET_PARAMETER", "POST"
    ].join(", ");

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .header("Public", &methods)
        .encode();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle ANNOUNCE request (SDP body with stream parameters)
fn handle_announce(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // Verify state
    if session.state() != SessionState::Connected {
        return error_result(StatusCode::METHOD_NOT_VALID, cseq);
    }

    // SDP parsing is handled by Section 38
    // Here we just acknowledge receipt

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: Some(SessionState::Announced),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle SETUP request
fn handle_setup(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // Parse Transport header
    let transport_str = match request.headers.get("Transport") {
        Some(t) => t,
        None => return error_result(StatusCode::BAD_REQUEST, cseq),
    };

    let client_transport = match TransportHeader::parse(transport_str) {
        Ok(t) => t,
        Err(_) => return error_result(StatusCode::BAD_REQUEST, cseq),
    };

    // Ports will be allocated by the session manager
    // Here we return placeholder that will be filled in by caller
    let ports = AllocatedPorts {
        audio_port: 0,  // Placeholder
        control_port: 0,
        timing_port: 0,
    };

    // Generate session ID
    let session_id = generate_session_id();

    // Build response Transport header
    let response_transport = client_transport.to_response_header(
        ports.audio_port,
        ports.control_port,
        ports.timing_port,
    );

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .session(&session_id)
        .header("Transport", &response_transport)
        .encode();

    HandleResult {
        response,
        new_state: Some(SessionState::Setup),
        allocated_ports: Some(ports),
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle RECORD request (start streaming)
fn handle_record(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    if session.state() != SessionState::Setup {
        return error_result(StatusCode::METHOD_NOT_VALID, cseq);
    }

    // Parse RTP-Info header for initial sequence/timestamp
    // Format: seq=<seq>;rtptime=<timestamp>
    let _rtp_info = request.headers.get("RTP-Info");

    // Report our audio latency (in samples at 44.1kHz)
    // 2 seconds = 88200 samples
    let latency_samples: u32 = 88200;

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .audio_latency(latency_samples)
        .encode();

    HandleResult {
        response,
        new_state: Some(SessionState::Streaming),
        allocated_ports: None,
        start_streaming: true,
        stop_streaming: false,
    }
}

/// Handle PAUSE request
fn handle_pause(cseq: u32, session: &ReceiverSession) -> HandleResult {
    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: Some(SessionState::Paused),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,  // Keep session alive, just pause output
    }
}

/// Handle FLUSH request (clear buffer)
fn handle_flush(request: &RtspRequest, cseq: u32) -> HandleResult {
    // Parse RTP-Info for flush point
    // Format: rtptime=<timestamp>
    let _rtp_info = request.headers.get("RTP-Info");

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle TEARDOWN request
fn handle_teardown(cseq: u32, session: &ReceiverSession) -> HandleResult {
    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: Some(SessionState::Teardown),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: true,
    }
}

/// Handle GET_PARAMETER (keep-alive, status queries)
fn handle_get_parameter(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // Body may contain parameter names to query
    // Empty body = keep-alive ping

    let body_str = String::from_utf8_lossy(&request.body);

    let response_body = if body_str.contains("volume") {
        format!("volume: {:.6}\r\n", session.volume())
    } else {
        String::new()
    };

    let response = if response_body.is_empty() {
        ResponseBuilder::ok().cseq(cseq).encode()
    } else {
        ResponseBuilder::ok()
            .cseq(cseq)
            .text_body(&response_body)
            .encode()
    };

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle SET_PARAMETER (volume, metadata, etc.)
fn handle_set_parameter(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // Content-Type determines what's being set
    let content_type = request.headers.get("Content-Type")
        .map(|s| s.as_str())
        .unwrap_or("");

    // Delegate to appropriate handler based on content type
    // Section 43 handles the detailed parsing

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle POST (pairing, auth)
fn handle_post(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // POST is used for pairing endpoints like /pair-setup, /pair-verify
    // For now, return not implemented

    let response = ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle unknown method
fn handle_unknown(cseq: u32) -> HandleResult {
    error_result(StatusCode::METHOD_NOT_ALLOWED, cseq)
}

/// Generate an error result
fn error_result(status: StatusCode, cseq: u32) -> HandleResult {
    let response = ResponseBuilder::error(status)
        .cseq(cseq)
        .encode();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Generate a random session ID
fn generate_session_id() -> String {
    use rand::Rng;
    let id: u64 = rand::thread_rng().gen();
    format!("{:016X}", id)
}
```

---

## Unit Tests

### 36.5 Unit Tests

- [ ] **36.5.1** Comprehensive codec tests

**File:** `src/protocol/rtsp/server_codec.rs` (test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_options_request() {
        let mut codec = RtspServerCodec::new();
        codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");

        let request = codec.decode().unwrap().unwrap();
        assert_eq!(request.method, Method::Options);
        assert_eq!(request.uri, "*");
        assert_eq!(request.headers.cseq(), Some(1));
    }

    #[test]
    fn test_parse_announce_with_sdp() {
        let sdp = "v=0\r\no=- 0 0 IN IP4 192.168.1.100\r\ns=AirTunes\r\n";
        let request_str = format!(
            "ANNOUNCE rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
             CSeq: 2\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            sdp.len(),
            sdp
        );

        let mut codec = RtspServerCodec::new();
        codec.feed(request_str.as_bytes());

        let request = codec.decode().unwrap().unwrap();
        assert_eq!(request.method, Method::Announce);
        assert_eq!(request.headers.get("Content-Type"), Some(&"application/sdp".to_string()));
        assert_eq!(String::from_utf8_lossy(&request.body), sdp);
    }

    #[test]
    fn test_parse_setup_request() {
        let request_str =
            "SETUP rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
             CSeq: 3\r\n\
             Transport: RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002\r\n\
             \r\n";

        let mut codec = RtspServerCodec::new();
        codec.feed(request_str.as_bytes());

        let request = codec.decode().unwrap().unwrap();
        assert_eq!(request.method, Method::Setup);

        let transport = TransportHeader::parse(
            request.headers.get("Transport").unwrap()
        ).unwrap();

        assert_eq!(transport.control_port, Some(6001));
        assert_eq!(transport.timing_port, Some(6002));
    }

    #[test]
    fn test_parse_incomplete_request() {
        let mut codec = RtspServerCodec::new();
        codec.feed(b"OPTIONS * RTSP/1.0\r\n");

        // Should return None (incomplete)
        assert!(codec.decode().unwrap().is_none());

        // Add rest of headers
        codec.feed(b"CSeq: 1\r\n\r\n");

        // Now should parse
        let request = codec.decode().unwrap().unwrap();
        assert_eq!(request.method, Method::Options);
    }

    #[test]
    fn test_parse_incomplete_body() {
        let mut codec = RtspServerCodec::new();
        codec.feed(
            b"SET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
              CSeq: 5\r\n\
              Content-Length: 20\r\n\
              \r\n\
              volume: -1"  // Only 10 bytes, need 20
        );

        // Should return None (incomplete body)
        assert!(codec.decode().unwrap().is_none());

        // Add rest of body
        codec.feed(b"5.000000\r\n");

        let request = codec.decode().unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&request.body), "volume: -15.000000\r\n");
    }

    #[test]
    fn test_parse_multiple_requests() {
        let mut codec = RtspServerCodec::new();
        codec.feed(
            b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n\
              OPTIONS * RTSP/1.0\r\nCSeq: 2\r\n\r\n"
        );

        let req1 = codec.decode().unwrap().unwrap();
        assert_eq!(req1.headers.cseq(), Some(1));

        let req2 = codec.decode().unwrap().unwrap();
        assert_eq!(req2.headers.cseq(), Some(2));

        // No more requests
        assert!(codec.decode().unwrap().is_none());
    }

    #[test]
    fn test_response_builder() {
        let response = ResponseBuilder::ok()
            .cseq(5)
            .session("ABC123")
            .header("Custom-Header", "value")
            .encode();

        let response_str = String::from_utf8(response).unwrap();
        assert!(response_str.starts_with("RTSP/1.0 200 OK\r\n"));
        assert!(response_str.contains("CSeq: 5\r\n"));
        assert!(response_str.contains("Session: ABC123\r\n"));
        assert!(response_str.contains("Custom-Header: value\r\n"));
        assert!(response_str.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_response_with_body() {
        let body = "volume: -15.000000\r\n";
        let response = ResponseBuilder::ok()
            .cseq(10)
            .text_body(body)
            .encode();

        let response_str = String::from_utf8(response).unwrap();
        assert!(response_str.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(response_str.contains("Content-Type: text/parameters\r\n"));
        assert!(response_str.ends_with(body));
    }

    #[test]
    fn test_error_response() {
        let response = ResponseBuilder::error(StatusCode::NOT_FOUND)
            .cseq(99)
            .encode();

        let response_str = String::from_utf8(response).unwrap();
        assert!(response_str.starts_with("RTSP/1.0 404 Not Found\r\n"));
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn test_parse_basic_transport() {
        let transport = TransportHeader::parse(
            "RTP/AVP/UDP;unicast;mode=record"
        ).unwrap();

        assert_eq!(transport.protocol, "RTP/AVP");
        assert_eq!(transport.lower_transport, LowerTransport::Udp);
        assert_eq!(transport.cast, CastMode::Unicast);
        assert_eq!(transport.mode, Some("record".to_string()));
    }

    #[test]
    fn test_parse_transport_with_ports() {
        let transport = TransportHeader::parse(
            "RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002"
        ).unwrap();

        assert_eq!(transport.control_port, Some(6001));
        assert_eq!(transport.timing_port, Some(6002));
    }

    #[test]
    fn test_parse_tcp_transport() {
        let transport = TransportHeader::parse(
            "RTP/AVP/TCP;unicast;interleaved=0-1"
        ).unwrap();

        assert_eq!(transport.lower_transport, LowerTransport::Tcp);
        assert_eq!(transport.interleaved, Some((0, 1)));
    }

    #[test]
    fn test_response_header_generation() {
        let transport = TransportHeader::parse(
            "RTP/AVP/UDP;unicast;mode=record"
        ).unwrap();

        let response = transport.to_response_header(6000, 6001, 6002);
        assert!(response.contains("server_port=6000"));
        assert!(response.contains("control_port=6001"));
        assert!(response.contains("timing_port=6002"));
    }
}
```

---

## Integration Tests

### 36.6 Integration Tests

- [ ] **36.6.1** Full RTSP conversation tests

**File:** `tests/protocol/rtsp_server_tests.rs`

```rust
//! Integration tests for RTSP server codec
//!
//! These tests simulate complete RTSP conversations between
//! a mock sender and our server codec.

use airplay2::protocol::rtsp::{RtspServerCodec, Method};
use airplay2::receiver::rtsp_handler::{handle_request, HandleResult};
use airplay2::receiver::session::{ReceiverSession, SessionState};

/// Simulate a complete RAOP session negotiation
#[test]
fn test_complete_session_negotiation() {
    let mut codec = RtspServerCodec::new();
    let mut session = ReceiverSession::new();

    // Step 1: OPTIONS
    codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(result.response).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Public:"));
    assert!(response_str.contains("ANNOUNCE"));

    // Step 2: ANNOUNCE with SDP
    let sdp = "v=0\r\n\
               o=iTunes 1234 0 IN IP4 192.168.1.100\r\n\
               s=iTunes\r\n\
               c=IN IP4 192.168.1.1\r\n\
               t=0 0\r\n\
               m=audio 0 RTP/AVP 96\r\n\
               a=rtpmap:96 AppleLossless\r\n\
               a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n";

    let announce = format!(
        "ANNOUNCE rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
         CSeq: 2\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {}\r\n\
         \r\n{}",
        sdp.len(),
        sdp
    );

    codec.clear();
    codec.feed(announce.as_bytes());
    session.set_state(SessionState::Connected);

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(String::from_utf8(result.response).unwrap().contains("200 OK"));
    assert_eq!(result.new_state, Some(SessionState::Announced));

    // Step 3: SETUP
    session.set_state(SessionState::Announced);
    codec.clear();
    codec.feed(
        b"SETUP rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 3\r\n\
          Transport: RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(result.response).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Session:"));
    assert!(response_str.contains("Transport:"));
    assert_eq!(result.new_state, Some(SessionState::Setup));
    assert!(result.allocated_ports.is_some());

    // Step 4: RECORD
    session.set_state(SessionState::Setup);
    codec.clear();
    codec.feed(
        b"RECORD rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 4\r\n\
          Range: npt=0-\r\n\
          RTP-Info: seq=1;rtptime=0\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(result.response).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Audio-Latency:"));
    assert!(result.start_streaming);

    // Step 5: TEARDOWN
    session.set_state(SessionState::Streaming);
    codec.clear();
    codec.feed(
        b"TEARDOWN rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 5\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(String::from_utf8(result.response).unwrap().contains("200 OK"));
    assert!(result.stop_streaming);
}

/// Test volume control via SET_PARAMETER
#[test]
fn test_volume_control() {
    let mut codec = RtspServerCodec::new();
    let session = ReceiverSession::new();

    let volume_cmd = "SET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
                      CSeq: 10\r\n\
                      Content-Type: text/parameters\r\n\
                      Content-Length: 20\r\n\
                      \r\n\
                      volume: -15.000000\r\n";

    codec.feed(volume_cmd.as_bytes());
    let request = codec.decode().unwrap().unwrap();

    assert_eq!(request.method, Method::SetParameter);
    let result = handle_request(&request, &session);

    assert!(String::from_utf8(result.response).unwrap().contains("200 OK"));
}

/// Test keep-alive via empty GET_PARAMETER
#[test]
fn test_keepalive() {
    let mut codec = RtspServerCodec::new();
    let session = ReceiverSession::new();

    codec.feed(
        b"GET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 20\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(String::from_utf8(result.response).unwrap().contains("200 OK"));
}

/// Test FLUSH handling
#[test]
fn test_flush() {
    let mut codec = RtspServerCodec::new();
    let session = ReceiverSession::new();

    codec.feed(
        b"FLUSH rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 30\r\n\
          RTP-Info: rtptime=12345\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(String::from_utf8(result.response).unwrap().contains("200 OK"));
}

/// Test error handling for invalid state transitions
#[test]
fn test_invalid_state_transition() {
    let mut codec = RtspServerCodec::new();
    let session = ReceiverSession::new();
    // Session is in initial state, not Setup

    codec.feed(
        b"RECORD rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 1\r\n\
          \r\n"
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    // Should get 455 Method Not Valid in This State
    assert!(String::from_utf8(result.response).unwrap().contains("455"));
}
```

---

## Acceptance Criteria

- [ ] RTSP requests parsed correctly (all methods)
- [ ] Incomplete requests return `Ok(None)` not error
- [ ] Binary and text bodies handled correctly
- [ ] Content-Length validated and enforced
- [ ] Response builder generates valid RTSP responses
- [ ] Transport header parsed and generated correctly
- [ ] CSeq properly echoed in responses
- [ ] Session ID generated and tracked
- [ ] State machine enforces valid transitions
- [ ] All RAOP-required methods implemented
- [ ] All unit tests pass
- [ ] Integration tests pass

---

## Notes

- **Reuse**: The client's `RtspRequest` and `RtspResponse` types are reused; only parsing direction differs
- **Sans-IO**: No async/await or networking in this module; that's handled by the connection layer
- **Error codes**: RTSP uses status codes similar to HTTP; we support the RAOP-relevant subset
- **Body handling**: Most RAOP bodies are small (SDP, parameters), but metadata can be larger
- **Future**: POST handling for pairing will be expanded when implementing password protection

---

## References

- [RFC 2326](https://tools.ietf.org/html/rfc2326) - Real Time Streaming Protocol (RTSP)
- [Unofficial AirPlay Spec - RTSP](https://nto.github.io/AirPlay.html#audio-rtsp)
- [shairport-sync RTSP handling](https://github.com/mikebrady/shairport-sync/blob/master/rtsp.c)
