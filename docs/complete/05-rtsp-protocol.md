# Section 05: RTSP Protocol (Sans-IO)

> **VERIFIED**: Checked against `src/protocol/rtsp/mod.rs` and submodules on 2025-01-30.
> Implementation includes additional methods (Get, SetRateAnchorTime) and modules
> (server_codec, transport) beyond original spec.

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 03**: Binary Plist Codec (must be complete)

## Overview

AirPlay 2 uses RTSP (Real Time Streaming Protocol) as its control protocol. This section implements a sans-IO RTSP codec that handles encoding requests, decoding responses, and managing session state without performing any I/O operations.

The AirPlay variant of RTSP extends standard RTSP with:
- Binary plist bodies
- Custom methods (SETUP, RECORD, SET_PARAMETER, GET_PARAMETER, FLUSH, TEARDOWN)
- CSeq sequence tracking
- Session management

## Objectives

- Implement sans-IO RTSP request encoder
- Implement sans-IO RTSP response decoder
- Handle incremental/streaming parsing
- Manage RTSP session state machine
- Support AirPlay-specific extensions

---

## Tasks

### 5.1 RTSP Types

- [x] **5.1.1** Define RTSP method enum

**File:** `src/protocol/rtsp/mod.rs`

```rust
//! Sans-IO RTSP protocol implementation for AirPlay

pub mod codec;
#[cfg(test)]
mod codec_tests;
#[cfg(test)]
mod compliance_tests;
pub mod headers;
#[cfg(test)]
mod headers_tests;
pub mod request;
#[cfg(test)]
mod request_tests;
pub mod response;
#[cfg(test)]
mod response_tests;
pub mod server_codec;  // Server-side codec for receiver mode
#[cfg(test)]
mod server_codec_tests;
pub mod session;
#[cfg(test)]
mod session_tests;
pub mod transport;  // Transport parameter parsing
#[cfg(test)]
mod transport_tests;

pub use codec::{RtspCodec, RtspCodecError};
pub use headers::Headers;
pub use request::{RtspRequest, RtspRequestBuilder};
pub use response::{RtspResponse, StatusCode};
pub use session::{RtspSession, SessionState};

/// RTSP methods used in AirPlay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Initiate session options negotiation
    Options,
    /// Announce stream information (SDP)
    Announce,
    /// Set up transport and session
    Setup,
    /// Start recording/streaming
    Record,
    /// Play (URL-based streaming)
    Play,
    /// Pause playback
    Pause,
    /// Flush buffers
    Flush,
    /// Tear down session
    Teardown,
    /// Set parameter (volume, progress, etc.)
    SetParameter,
    /// Get parameter (playback info, etc.)
    GetParameter,
    /// POST for pairing/auth
    Post,
    /// GET for info
    Get,
    /// Set playback rate and anchor time
    SetRateAnchorTime,
}

impl Method {
    /// Convert to RTSP method string
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Options => "OPTIONS",
            Method::Announce => "ANNOUNCE",
            Method::Setup => "SETUP",
            Method::Record => "RECORD",
            Method::Play => "PLAY",
            Method::Pause => "PAUSE",
            Method::Flush => "FLUSH",
            Method::Teardown => "TEARDOWN",
            Method::SetParameter => "SET_PARAMETER",
            Method::GetParameter => "GET_PARAMETER",
            Method::Post => "POST",
            Method::Get => "GET",
            Method::SetRateAnchorTime => "SETRATEANCHORTIME",
        }
    }
}

impl std::str::FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "OPTIONS" => Ok(Method::Options),
            "ANNOUNCE" => Ok(Method::Announce),
            "SETUP" => Ok(Method::Setup),
            "RECORD" => Ok(Method::Record),
            "PLAY" => Ok(Method::Play),
            "PAUSE" => Ok(Method::Pause),
            "FLUSH" => Ok(Method::Flush),
            "TEARDOWN" => Ok(Method::Teardown),
            "SET_PARAMETER" => Ok(Method::SetParameter),
            "GET_PARAMETER" => Ok(Method::GetParameter),
            "POST" => Ok(Method::Post),
            "GET" => Ok(Method::Get),
            "SETRATEANCHORTIME" => Ok(Method::SetRateAnchorTime),
            _ => Err(()),
        }
    }
}
```

- [x] **5.1.2** Define RTSP headers container

**File:** `src/protocol/rtsp/headers.rs`

```rust
use std::collections::HashMap;

/// Well-known RTSP header names
pub mod names {
    pub const CSEQ: &str = "CSeq";
    pub const CONTENT_TYPE: &str = "Content-Type";
    pub const CONTENT_LENGTH: &str = "Content-Length";
    pub const SESSION: &str = "Session";
    pub const TRANSPORT: &str = "Transport";
    pub const USER_AGENT: &str = "User-Agent";
    pub const ACTIVE_REMOTE: &str = "Active-Remote";
    pub const DACP_ID: &str = "DACP-ID";
    pub const CLIENT_INSTANCE: &str = "Client-Instance";
    pub const X_APPLE_DEVICE_ID: &str = "X-Apple-Device-ID";
    pub const X_APPLE_SESSION_ID: &str = "X-Apple-Session-ID";
    pub const X_APPLE_PROTOCOL_VERSION: &str = "X-Apple-ProtocolVersion";
}

/// RTSP header collection
#[derive(Debug, Clone, Default)]
pub struct Headers {
    inner: HashMap<String, String>,
}

impl Headers {
    /// Create empty headers
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a header (case-insensitive key storage)
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.inner.insert(name.into(), value.into());
    }

    /// Get header value (case-insensitive)
    pub fn get(&self, name: &str) -> Option<&str> {
        // RTSP headers are case-insensitive
        self.inner
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Check if header exists
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Get CSeq value
    pub fn cseq(&self) -> Option<u32> {
        self.get(names::CSEQ)?.parse().ok()
    }

    /// Get Content-Length value
    pub fn content_length(&self) -> Option<usize> {
        self.get(names::CONTENT_LENGTH)?.parse().ok()
    }

    /// Get Content-Type value
    pub fn content_type(&self) -> Option<&str> {
        self.get(names::CONTENT_TYPE)
    }

    /// Get Session ID
    pub fn session(&self) -> Option<&str> {
        self.get(names::SESSION)
    }

    /// Iterate over all headers
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of headers
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl FromIterator<(String, String)> for Headers {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}
```

---

### 5.2 RTSP Request

- [x] **5.2.1** Implement RTSP request structure

**File:** `src/protocol/rtsp/request.rs`

```rust
use super::{Headers, Method, headers::names};

/// An RTSP request message
#[derive(Debug, Clone)]
pub struct RtspRequest {
    /// HTTP method
    pub method: Method,
    /// Request URI (e.g., "rtsp://192.168.1.10/1234567")
    pub uri: String,
    /// Request headers
    pub headers: Headers,
    /// Request body (may be empty)
    pub body: Vec<u8>,
}

impl RtspRequest {
    /// Create a new request
    pub fn new(method: Method, uri: impl Into<String>) -> Self {
        Self {
            method,
            uri: uri.into(),
            headers: Headers::new(),
            body: Vec::new(),
        }
    }

    /// Create a builder for constructing requests
    pub fn builder(method: Method, uri: impl Into<String>) -> RtspRequestBuilder {
        RtspRequestBuilder::new(method, uri)
    }

    /// Encode request to bytes
    ///
    /// Returns the complete RTSP request ready for transmission
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(256 + self.body.len());

        // Request line: METHOD uri RTSP/1.0\r\n
        output.extend_from_slice(self.method.as_str().as_bytes());
        output.push(b' ');
        output.extend_from_slice(self.uri.as_bytes());
        output.extend_from_slice(b" RTSP/1.0\r\n");

        // Headers
        for (name, value) in self.headers.iter() {
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }

        // Content-Length if body present
        if !self.body.is_empty() {
            let len_header = format!("{}: {}\r\n", names::CONTENT_LENGTH, self.body.len());
            output.extend_from_slice(len_header.as_bytes());
        }

        // End of headers
        output.extend_from_slice(b"\r\n");

        // Body
        output.extend_from_slice(&self.body);

        output
    }
}

/// Builder for RTSP requests
#[derive(Debug)]
pub struct RtspRequestBuilder {
    request: RtspRequest,
}

impl RtspRequestBuilder {
    /// Create a new builder
    pub fn new(method: Method, uri: impl Into<String>) -> Self {
        Self {
            request: RtspRequest::new(method, uri),
        }
    }

    /// Add a header
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.request.headers.insert(name, value);
        self
    }

    /// Set CSeq header
    pub fn cseq(self, seq: u32) -> Self {
        self.header(names::CSEQ, seq.to_string())
    }

    /// Set Content-Type header
    pub fn content_type(self, content_type: &str) -> Self {
        self.header(names::CONTENT_TYPE, content_type)
    }

    /// Set User-Agent header
    pub fn user_agent(self, agent: &str) -> Self {
        self.header(names::USER_AGENT, agent)
    }

    /// Set session ID header
    pub fn session(self, session_id: &str) -> Self {
        self.header(names::SESSION, session_id)
    }

    /// Set body as raw bytes
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.request.body = body;
        self
    }

    /// Set body as binary plist
    pub fn body_plist(mut self, plist: &crate::protocol::plist::PlistValue) -> Self {
        self.request.body = crate::protocol::plist::encode(plist)
            .expect("plist encoding should not fail");
        self.request.headers.insert(
            names::CONTENT_TYPE,
            "application/x-apple-binary-plist".to_string(),
        );
        self
    }

    /// Build the request
    pub fn build(self) -> RtspRequest {
        self.request
    }
}
```

---

### 5.3 RTSP Response

- [x] **5.3.1** Implement RTSP response structure

**File:** `src/protocol/rtsp/response.rs`

```rust
use super::Headers;

/// RTSP status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(pub u16);

impl StatusCode {
    pub const OK: StatusCode = StatusCode(200);
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    pub const NOT_ACCEPTABLE: StatusCode = StatusCode(406);
    pub const INTERNAL_ERROR: StatusCode = StatusCode(500);
    pub const NOT_IMPLEMENTED: StatusCode = StatusCode(501);
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);

    /// Check if this is a success status (2xx)
    pub fn is_success(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// Get status code as u16
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

/// An RTSP response message
#[derive(Debug, Clone)]
pub struct RtspResponse {
    /// RTSP version (usually "RTSP/1.0")
    pub version: String,
    /// Status code
    pub status: StatusCode,
    /// Reason phrase (e.g., "OK")
    pub reason: String,
    /// Response headers
    pub headers: Headers,
    /// Response body (may be empty)
    pub body: Vec<u8>,
}

impl RtspResponse {
    /// Check if response indicates success
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Get CSeq from response
    pub fn cseq(&self) -> Option<u32> {
        self.headers.cseq()
    }

    /// Get session ID from response
    pub fn session(&self) -> Option<&str> {
        self.headers.session()
    }

    /// Parse body as binary plist
    pub fn body_as_plist(&self) -> Result<crate::protocol::plist::PlistValue, crate::protocol::plist::PlistDecodeError> {
        crate::protocol::plist::decode(&self.body)
    }

    /// Check if body is binary plist
    pub fn is_plist(&self) -> bool {
        self.headers
            .content_type()
            .map(|ct| ct.contains("apple-binary-plist") || ct.contains("application/x-plist"))
            .unwrap_or(false)
    }
}
```

---

### 5.4 Sans-IO Codec

- [x] **5.4.1** Implement the RTSP codec for encoding/decoding

**File:** `src/protocol/rtsp/codec.rs`

```rust
use super::{Headers, RtspResponse, StatusCode};
use thiserror::Error;

/// Errors during RTSP parsing
#[derive(Debug, Error)]
pub enum RtspCodecError {
    #[error("incomplete data: need more bytes")]
    Incomplete,

    #[error("invalid status line: {0}")]
    InvalidStatusLine(String),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("invalid content length")]
    InvalidContentLength,

    #[error("response too large: {size} bytes")]
    ResponseTooLarge { size: usize },
}

/// Sans-IO RTSP codec for parsing responses
///
/// This codec handles incremental parsing of RTSP responses.
/// Feed bytes with `feed()`, check for complete responses with `decode()`.
pub struct RtspCodec {
    /// Internal buffer for partial data
    buffer: Vec<u8>,
    /// Maximum response size (default 1MB)
    max_size: usize,
    /// Parser state
    state: ParseState,
}

#[derive(Debug, Clone)]
enum ParseState {
    /// Waiting for status line
    StatusLine,
    /// Parsing headers
    Headers {
        version: String,
        status: StatusCode,
        reason: String,
    },
    /// Reading body
    Body {
        version: String,
        status: StatusCode,
        reason: String,
        headers: Headers,
        content_length: usize,
    },
}

impl RtspCodec {
    /// Create a new codec
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            max_size: 1024 * 1024, // 1MB
            state: ParseState::StatusLine,
        }
    }

    /// Set maximum response size
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }

    /// Feed bytes into the codec
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), RtspCodecError> {
        if self.buffer.len() + bytes.len() > self.max_size {
            return Err(RtspCodecError::ResponseTooLarge {
                size: self.buffer.len() + bytes.len(),
            });
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// Try to decode a complete response
    ///
    /// Returns `Ok(Some(response))` if a complete response is available,
    /// `Ok(None)` if more data is needed, or an error if parsing fails.
    pub fn decode(&mut self) -> Result<Option<RtspResponse>, RtspCodecError> {
        loop {
            match &self.state {
                ParseState::StatusLine => {
                    if let Some(line_end) = self.find_line_end() {
                        let line = String::from_utf8_lossy(&self.buffer[..line_end]).to_string();
                        let (version, status, reason) = Self::parse_status_line(&line)?;

                        // Remove parsed line from buffer
                        self.buffer.drain(..line_end + 2);

                        self.state = ParseState::Headers {
                            version,
                            status,
                            reason,
                        };
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::Headers {
                    version,
                    status,
                    reason,
                } => {
                    if let Some((headers, body_start)) = self.parse_headers()? {
                        let content_length = headers.content_length().unwrap_or(0);

                        // Remove headers from buffer
                        self.buffer.drain(..body_start);

                        if content_length == 0 {
                            // No body, response complete
                            let response = RtspResponse {
                                version: version.clone(),
                                status: *status,
                                reason: reason.clone(),
                                headers,
                                body: Vec::new(),
                            };
                            self.state = ParseState::StatusLine;
                            return Ok(Some(response));
                        }

                        self.state = ParseState::Body {
                            version: version.clone(),
                            status: *status,
                            reason: reason.clone(),
                            headers,
                            content_length,
                        };
                    } else {
                        return Ok(None);
                    }
                }

                ParseState::Body {
                    version,
                    status,
                    reason,
                    headers,
                    content_length,
                } => {
                    if self.buffer.len() >= *content_length {
                        let body = self.buffer.drain(..*content_length).collect();

                        let response = RtspResponse {
                            version: version.clone(),
                            status: *status,
                            reason: reason.clone(),
                            headers: headers.clone(),
                            body,
                        };

                        self.state = ParseState::StatusLine;
                        return Ok(Some(response));
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Clear the codec buffer and reset state
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state = ParseState::StatusLine;
    }

    /// Get current buffer length
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    // Helper methods

    fn find_line_end(&self) -> Option<usize> {
        self.buffer
            .windows(2)
            .position(|w| w == b"\r\n")
    }

    fn parse_status_line(line: &str) -> Result<(String, StatusCode, String), RtspCodecError> {
        // Format: "RTSP/1.0 200 OK"
        let mut parts = line.splitn(3, ' ');

        let version = parts
            .next()
            .ok_or_else(|| RtspCodecError::InvalidStatusLine(line.to_string()))?
            .to_string();

        let status = parts
            .next()
            .ok_or_else(|| RtspCodecError::InvalidStatusLine(line.to_string()))?
            .parse::<u16>()
            .map_err(|_| RtspCodecError::InvalidStatusLine(line.to_string()))?;

        let reason = parts.next().unwrap_or("").to_string();

        Ok((version, StatusCode(status), reason))
    }

    fn parse_headers(&self) -> Result<Option<(Headers, usize)>, RtspCodecError> {
        // Find end of headers (blank line)
        let header_end = self.buffer
            .windows(4)
            .position(|w| w == b"\r\n\r\n");

        let header_end = match header_end {
            Some(pos) => pos,
            None => return Ok(None),
        };

        let header_bytes = &self.buffer[..header_end];
        let header_str = String::from_utf8_lossy(header_bytes);

        let mut headers = Headers::new();

        for line in header_str.split("\r\n") {
            if line.is_empty() {
                continue;
            }

            let colon_pos = line
                .find(':')
                .ok_or_else(|| RtspCodecError::InvalidHeader(line.to_string()))?;

            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();

            headers.insert(name, value);
        }

        // +4 for the \r\n\r\n
        Ok(Some((headers, header_end + 4)))
    }
}

impl Default for RtspCodec {
    fn default() -> Self {
        Self::new()
    }
}
```

---

### 5.5 Session State Machine

- [x] **5.5.1** Implement RTSP session state management

**File:** `src/protocol/rtsp/session.rs`

```rust
use super::{Method, RtspRequest, RtspRequestBuilder, RtspResponse, headers::names};

/// RTSP session states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state, no session established
    Init,
    /// OPTIONS exchanged, ready for setup
    Ready,
    /// SETUP complete, transport configured
    Setup,
    /// RECORD/PLAY started, streaming active
    Playing,
    /// Paused
    Paused,
    /// Session terminated
    Terminated,
}

/// RTSP session manager (sans-IO)
///
/// Manages session state, CSeq numbering, and session ID tracking.
pub struct RtspSession {
    /// Current session state
    state: SessionState,
    /// CSeq counter
    cseq: u32,
    /// Session ID (from server)
    session_id: Option<String>,
    /// Our device ID
    device_id: String,
    /// Our session ID (generated)
    client_session_id: String,
    /// Base URI for requests
    base_uri: String,
    /// User agent string
    user_agent: String,
}

impl RtspSession {
    /// Create a new session
    pub fn new(device_address: &str, port: u16) -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let device_id: u64 = rng.gen();
        let session_id: u64 = rng.gen();

        Self {
            state: SessionState::Init,
            cseq: 0,
            session_id: None,
            device_id: format!("{:016X}", device_id),
            client_session_id: format!("{:016X}", session_id),
            base_uri: format!("rtsp://{}:{}", device_address, port),
            user_agent: format!("airplay2-rs/{}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// Get current session state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get server session ID (if established)
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Get next CSeq and increment counter
    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    /// Create base request builder with common headers
    fn request_builder(&mut self, method: Method, path: &str) -> RtspRequestBuilder {
        let uri = if path.starts_with('/') {
            format!("{}{}", self.base_uri, path)
        } else {
            format!("{}/{}", self.base_uri, path)
        };

        let mut builder = RtspRequest::builder(method, uri)
            .cseq(self.next_cseq())
            .user_agent(&self.user_agent)
            .header(names::X_APPLE_DEVICE_ID, &self.device_id)
            .header(names::X_APPLE_SESSION_ID, &self.client_session_id);

        if let Some(ref session) = self.session_id {
            builder = builder.session(session);
        }

        builder
    }

    /// Create OPTIONS request
    pub fn options_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Options, "*")
            .build()
    }

    /// Create SETUP request
    pub fn setup_request(&mut self, transport_params: &str) -> RtspRequest {
        self.request_builder(Method::Setup, "")
            .header(names::TRANSPORT, transport_params)
            .build()
    }

    /// Create RECORD request
    pub fn record_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Record, "")
            .header("Range", "npt=0-")
            .header("RTP-Info", "seq=0;rtptime=0")
            .build()
    }

    /// Create SET_PARAMETER request
    pub fn set_parameter_request(
        &mut self,
        content_type: &str,
        body: Vec<u8>,
    ) -> RtspRequest {
        self.request_builder(Method::SetParameter, "")
            .content_type(content_type)
            .body(body)
            .build()
    }

    /// Create GET_PARAMETER request
    pub fn get_parameter_request(&mut self) -> RtspRequest {
        self.request_builder(Method::GetParameter, "")
            .build()
    }

    /// Create FLUSH request
    pub fn flush_request(&mut self, seq: u16, timestamp: u32) -> RtspRequest {
        self.request_builder(Method::Flush, "")
            .header("RTP-Info", format!("seq={};rtptime={}", seq, timestamp))
            .build()
    }

    /// Create TEARDOWN request
    pub fn teardown_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Teardown, "")
            .build()
    }

    /// Process a response and update session state
    ///
    /// Returns Ok(()) if response is valid, Err with description otherwise.
    pub fn process_response(
        &mut self,
        method: Method,
        response: &RtspResponse,
    ) -> Result<(), String> {
        // Validate CSeq matches (optional, for debugging)

        if !response.is_success() {
            return Err(format!(
                "{} failed: {} {}",
                method.as_str(),
                response.status.as_u16(),
                response.reason
            ));
        }

        // Extract session ID if present
        if let Some(session) = response.session() {
            // Session ID may have ";timeout=X" suffix
            let session_id = session.split(';').next().unwrap_or(session);
            self.session_id = Some(session_id.to_string());
        }

        // Update state based on method
        match method {
            Method::Options => {
                self.state = SessionState::Ready;
            }
            Method::Setup => {
                self.state = SessionState::Setup;
            }
            Method::Record | Method::Play => {
                self.state = SessionState::Playing;
            }
            Method::Pause => {
                self.state = SessionState::Paused;
            }
            Method::Teardown => {
                self.state = SessionState::Terminated;
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a method is valid in current state
    pub fn can_send(&self, method: Method) -> bool {
        match (self.state, method) {
            (SessionState::Init, Method::Options) => true,
            (SessionState::Init, Method::Post) => true, // For pairing
            (SessionState::Ready, Method::Setup) => true,
            (SessionState::Ready, Method::Post) => true,
            (SessionState::Setup, Method::Record) => true,
            (SessionState::Setup, Method::Play) => true,
            (SessionState::Playing, Method::Pause) => true,
            (SessionState::Playing, Method::Flush) => true,
            (SessionState::Playing, Method::SetParameter) => true,
            (SessionState::Playing, Method::GetParameter) => true,
            (SessionState::Playing, Method::Teardown) => true,
            (SessionState::Paused, Method::Record) => true,
            (SessionState::Paused, Method::Play) => true,
            (SessionState::Paused, Method::Teardown) => true,
            (SessionState::Paused, Method::SetParameter) => true,
            (_, Method::Options) => true, // OPTIONS always allowed
            (_, Method::Teardown) => true, // TEARDOWN always allowed
            _ => false,
        }
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/rtsp/request.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_encode_simple() {
        let request = RtspRequest::builder(Method::Options, "rtsp://192.168.1.10:7000/*")
            .cseq(1)
            .user_agent("test/1.0")
            .build();

        let encoded = request.encode();
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(encoded_str.starts_with("OPTIONS rtsp://192.168.1.10:7000/* RTSP/1.0\r\n"));
        assert!(encoded_str.contains("CSeq: 1\r\n"));
        assert!(encoded_str.contains("User-Agent: test/1.0\r\n"));
        assert!(encoded_str.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_request_encode_with_body() {
        let body = b"test body content".to_vec();
        let request = RtspRequest::builder(Method::SetParameter, "rtsp://example.com/")
            .cseq(5)
            .content_type("text/parameters")
            .body(body.clone())
            .build();

        let encoded = request.encode();
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(encoded_str.contains("Content-Type: text/parameters\r\n"));
        assert!(encoded_str.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(encoded_str.ends_with("test body content"));
    }

    #[test]
    fn test_method_as_str() {
        assert_eq!(Method::Options.as_str(), "OPTIONS");
        assert_eq!(Method::Setup.as_str(), "SETUP");
        assert_eq!(Method::SetParameter.as_str(), "SET_PARAMETER");
    }

    #[test]
    fn test_method_from_str() {
        use std::str::FromStr;
        assert_eq!(Method::from_str("OPTIONS"), Ok(Method::Options));
        assert_eq!(Method::from_str("options"), Ok(Method::Options));
        assert_eq!(Method::from_str("GET"), Ok(Method::Get));
        assert!(Method::from_str("INVALID").is_err());
    }
}
```

### Test File: `src/protocol/rtsp/codec.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_simple_response() {
        let mut codec = RtspCodec::new();

        codec.feed(b"RTSP/1.0 200 OK\r\n\
                     CSeq: 1\r\n\
                     \r\n").unwrap();

        let response = codec.decode().unwrap().unwrap();

        assert_eq!(response.version, "RTSP/1.0");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.reason, "OK");
        assert_eq!(response.cseq(), Some(1));
        assert!(response.body.is_empty());
    }

    #[test]
    fn test_decode_response_with_body() {
        let mut codec = RtspCodec::new();

        codec.feed(b"RTSP/1.0 200 OK\r\n\
                     CSeq: 2\r\n\
                     Content-Length: 5\r\n\
                     \r\n\
                     hello").unwrap();

        let response = codec.decode().unwrap().unwrap();

        assert_eq!(response.body, b"hello");
    }

    #[test]
    fn test_decode_incremental() {
        let mut codec = RtspCodec::new();

        // Feed partial data
        codec.feed(b"RTSP/1.0 200 ").unwrap();
        assert!(codec.decode().unwrap().is_none());

        codec.feed(b"OK\r\n").unwrap();
        assert!(codec.decode().unwrap().is_none());

        codec.feed(b"CSeq: 1\r\n\r\n").unwrap();
        assert!(codec.decode().unwrap().is_some());
    }

    #[test]
    fn test_decode_multiple_responses() {
        let mut codec = RtspCodec::new();

        codec.feed(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n\
                     RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n").unwrap();

        let r1 = codec.decode().unwrap().unwrap();
        assert_eq!(r1.cseq(), Some(1));

        let r2 = codec.decode().unwrap().unwrap();
        assert_eq!(r2.cseq(), Some(2));

        assert!(codec.decode().unwrap().is_none());
    }

    #[test]
    fn test_decode_invalid_status_line() {
        let mut codec = RtspCodec::new();

        codec.feed(b"INVALID LINE\r\n\r\n").unwrap();

        let result = codec.decode();
        assert!(matches!(result, Err(RtspCodecError::InvalidStatusLine(_))));
    }

    #[test]
    fn test_status_code_checks() {
        assert!(StatusCode::OK.is_success());
        assert!(!StatusCode::OK.is_client_error());

        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(!StatusCode::NOT_FOUND.is_success());

        assert!(StatusCode::INTERNAL_ERROR.is_server_error());
    }

    #[test]
    fn test_max_size_limit() {
        let mut codec = RtspCodec::new().with_max_size(100);

        let result = codec.feed(&[0u8; 200]);

        assert!(matches!(result, Err(RtspCodecError::ResponseTooLarge { .. })));
    }
}
```

### Test File: `src/protocol/rtsp/session.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_initial_state() {
        let session = RtspSession::new("192.168.1.10", 7000);

        assert_eq!(session.state(), SessionState::Init);
        assert!(session.session_id().is_none());
    }

    #[test]
    fn test_session_cseq_increments() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        let r1 = session.options_request();
        let r2 = session.options_request();

        assert_eq!(r1.headers.cseq(), Some(1));
        assert_eq!(r2.headers.cseq(), Some(2));
    }

    #[test]
    fn test_session_state_transitions() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        // Initial -> Ready via OPTIONS
        let response = RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers: Headers::new(),
            body: Vec::new(),
        };

        session.process_response(Method::Options, &response).unwrap();
        assert_eq!(session.state(), SessionState::Ready);
    }

    #[test]
    fn test_session_extracts_session_id() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        let mut headers = Headers::new();
        headers.insert("Session", "ABC123;timeout=60");

        let response = RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        };

        session.process_response(Method::Setup, &response).unwrap();

        assert_eq!(session.session_id(), Some("ABC123"));
    }

    #[test]
    fn test_session_can_send_validation() {
        let session = RtspSession::new("192.168.1.10", 7000);

        // In Init state
        assert!(session.can_send(Method::Options));
        assert!(!session.can_send(Method::Setup));
        assert!(!session.can_send(Method::Record));
    }

    #[test]
    fn test_request_includes_common_headers() {
        let mut session = RtspSession::new("192.168.1.10", 7000);
        let request = session.options_request();

        assert!(request.headers.get("X-Apple-Device-ID").is_some());
        assert!(request.headers.get("X-Apple-Session-ID").is_some());
        assert!(request.headers.get("User-Agent").is_some());
    }
}
```

---

## Integration Tests

### Test: Real AirPlay RTSP exchange patterns

```rust
// tests/protocol/rtsp_integration.rs

#[test]
fn test_full_session_flow() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let mut codec = RtspCodec::new();

    // 1. OPTIONS
    let options = session.options_request();
    assert!(options.encode().len() > 0);

    // Simulate response
    codec.feed(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: SETUP, RECORD\r\n\r\n").unwrap();
    let response = codec.decode().unwrap().unwrap();
    session.process_response(Method::Options, &response).unwrap();

    assert_eq!(session.state(), SessionState::Ready);

    // 2. SETUP
    let setup = session.setup_request("RTP/AVP/UDP;unicast;mode=record");
    assert!(setup.encode().len() > 0);

    // ... continue flow
}
```

---

## Acceptance Criteria

- [x] RTSP request encoding produces valid protocol messages
- [x] RTSP response decoder handles all status codes
- [x] Incremental parsing works correctly
- [x] Session state machine transitions correctly
- [x] CSeq numbering is sequential
- [x] Session ID extraction works with timeout suffix
- [x] Common AirPlay headers are included
- [x] Binary plist bodies are supported
- [x] All unit tests pass
- [x] Integration tests with real message patterns pass

---

## Notes

- RTSP parsing should be lenient (accept variations in whitespace, etc.)
- Consider adding request timeout tracking
- The session state machine may need refinement based on real device behavior
- Debug logging should show full request/response for protocol debugging
