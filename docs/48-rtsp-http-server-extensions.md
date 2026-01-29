# Section 48: RTSP/HTTP Server Extensions for AirPlay 2

## Dependencies
- **Section 36**: RTSP Server (Sans-IO) - existing server codec
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 03**: Binary Plist Codec (for body parsing)

## Overview

AirPlay 2 uses a hybrid RTSP/HTTP protocol where some endpoints behave like HTTP (POST to paths like `/pair-setup`) while others are traditional RTSP methods. This section extends our existing RTSP server codec to handle AirPlay 2-specific requests.

### Key Differences from AirPlay 1

| Aspect | AirPlay 1 (RAOP) | AirPlay 2 |
|--------|------------------|-----------|
| Body Format | SDP, text/parameters | Binary plist |
| Endpoints | RTSP methods only | RTSP + HTTP-style POST paths |
| Content-Type | application/sdp | application/x-apple-binary-plist |
| Authentication | In-band RSA | Separate pairing endpoints |

### AirPlay 2 Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/info` | GET | Query device capabilities |
| `/pair-setup` | POST | SRP pairing setup (M1-M4) |
| `/pair-verify` | POST | Session verification (M1-M4) |
| `/fp-setup` | POST | FairPlay setup (not implemented) |
| `/command` | POST | Playback commands |
| `/feedback` | POST | Feedback/status channel |
| `/audioMode` | POST | Audio mode configuration |
| Standard RTSP | Various | SETUP, RECORD, etc. |

## Objectives

- Extend RTSP server codec to handle HTTP-style endpoints
- Add binary plist body parsing for AirPlay 2 requests
- Implement request routing based on method and path
- Support both encrypted and plaintext request handling
- Maintain sans-IO design principles

---

## Tasks

### 48.1 Request Type Detection

- [ ] **48.1.1** Implement request type classification

**File:** `src/receiver/ap2/request_router.rs`

```rust
//! Request routing for AirPlay 2 receiver
//!
//! AirPlay 2 uses both RTSP methods and HTTP-style POST endpoints.
//! This module classifies incoming requests and routes them appropriately.

use crate::protocol::rtsp::{RtspRequest, Method};

/// Classification of AirPlay 2 requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ap2RequestType {
    /// Standard RTSP request (OPTIONS, SETUP, RECORD, etc.)
    Rtsp(RtspMethod),

    /// HTTP-style endpoint (POST to specific paths)
    Endpoint(Ap2Endpoint),

    /// Unknown/unsupported request
    Unknown,
}

/// RTSP methods used in AirPlay 2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtspMethod {
    Options,
    Setup,
    Record,
    Pause,
    Flush,
    Teardown,
    GetParameter,
    SetParameter,
    Get,  // Used for /info
}

/// HTTP-style endpoints in AirPlay 2
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ap2Endpoint {
    /// GET /info - device capabilities
    Info,

    /// POST /pair-setup - SRP pairing
    PairSetup,

    /// POST /pair-verify - session verification
    PairVerify,

    /// POST /fp-setup - FairPlay (not supported)
    FairPlaySetup,

    /// POST /command - playback commands
    Command,

    /// POST /feedback - status feedback
    Feedback,

    /// POST /audioMode - audio configuration
    AudioMode,

    /// POST /auth-setup - MFi authentication
    AuthSetup,

    /// Unknown endpoint
    Unknown(String),
}

impl Ap2RequestType {
    /// Classify an RTSP request
    pub fn classify(request: &RtspRequest) -> Self {
        match request.method {
            Method::Options => Self::Rtsp(RtspMethod::Options),
            Method::Setup => Self::Rtsp(RtspMethod::Setup),
            Method::Record => Self::Rtsp(RtspMethod::Record),
            Method::Pause => Self::Rtsp(RtspMethod::Pause),
            Method::Flush => Self::Rtsp(RtspMethod::Flush),
            Method::Teardown => Self::Rtsp(RtspMethod::Teardown),
            Method::GetParameter => Self::Rtsp(RtspMethod::GetParameter),
            Method::SetParameter => Self::Rtsp(RtspMethod::SetParameter),

            Method::Get => {
                // GET requests are routed by path
                Self::Endpoint(Self::classify_get_endpoint(&request.uri))
            }

            Method::Post => {
                // POST requests are routed by path
                Self::Endpoint(Self::classify_post_endpoint(&request.uri))
            }

            _ => Self::Unknown,
        }
    }

    fn classify_get_endpoint(uri: &str) -> Ap2Endpoint {
        // Extract path from URI (may be full URL or just path)
        let path = Self::extract_path(uri);

        match path {
            "/info" => Ap2Endpoint::Info,
            _ => Ap2Endpoint::Unknown(path.to_string()),
        }
    }

    fn classify_post_endpoint(uri: &str) -> Ap2Endpoint {
        let path = Self::extract_path(uri);

        match path {
            "/pair-setup" => Ap2Endpoint::PairSetup,
            "/pair-verify" => Ap2Endpoint::PairVerify,
            "/fp-setup" => Ap2Endpoint::FairPlaySetup,
            "/command" => Ap2Endpoint::Command,
            "/feedback" => Ap2Endpoint::Feedback,
            "/audioMode" => Ap2Endpoint::AudioMode,
            "/auth-setup" => Ap2Endpoint::AuthSetup,
            _ => Ap2Endpoint::Unknown(path.to_string()),
        }
    }

    fn extract_path(uri: &str) -> &str {
        // Handle both "rtsp://host/path" and "/path" formats
        if let Some(idx) = uri.find("://") {
            // Full URL: find path after host
            let after_scheme = &uri[idx + 3..];
            if let Some(path_idx) = after_scheme.find('/') {
                &after_scheme[path_idx..]
            } else {
                "/"
            }
        } else {
            // Just the path
            uri
        }
    }
}

impl Ap2Endpoint {
    /// Check if this endpoint requires authentication
    pub fn requires_auth(&self) -> bool {
        match self {
            // Pairing endpoints don't require prior auth
            Self::Info => false,
            Self::PairSetup => false,
            Self::PairVerify => false,
            Self::AuthSetup => false,

            // Everything else requires completed pairing
            Self::FairPlaySetup => true,
            Self::Command => true,
            Self::Feedback => true,
            Self::AudioMode => true,
            Self::Unknown(_) => true,
        }
    }

    /// Check if this endpoint accepts binary plist bodies
    pub fn expects_bplist(&self) -> bool {
        match self {
            Self::Info => false,
            Self::PairSetup => true,
            Self::PairVerify => true,
            Self::FairPlaySetup => true,
            Self::Command => true,
            Self::Feedback => true,
            Self::AudioMode => true,
            Self::AuthSetup => true,
            Self::Unknown(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rtsp_methods() {
        let request = RtspRequest {
            method: Method::Setup,
            uri: "rtsp://192.168.1.1/12345".to_string(),
            headers: Default::default(),
            body: vec![],
        };

        assert_eq!(
            Ap2RequestType::classify(&request),
            Ap2RequestType::Rtsp(RtspMethod::Setup)
        );
    }

    #[test]
    fn test_classify_post_endpoints() {
        let request = RtspRequest {
            method: Method::Post,
            uri: "/pair-setup".to_string(),
            headers: Default::default(),
            body: vec![],
        };

        assert_eq!(
            Ap2RequestType::classify(&request),
            Ap2RequestType::Endpoint(Ap2Endpoint::PairSetup)
        );
    }

    #[test]
    fn test_classify_get_info() {
        let request = RtspRequest {
            method: Method::Get,
            uri: "rtsp://192.168.1.100:7000/info".to_string(),
            headers: Default::default(),
            body: vec![],
        };

        assert_eq!(
            Ap2RequestType::classify(&request),
            Ap2RequestType::Endpoint(Ap2Endpoint::Info)
        );
    }

    #[test]
    fn test_auth_requirements() {
        assert!(!Ap2Endpoint::PairSetup.requires_auth());
        assert!(!Ap2Endpoint::PairVerify.requires_auth());
        assert!(Ap2Endpoint::Command.requires_auth());
        assert!(Ap2Endpoint::Feedback.requires_auth());
    }
}
```

---

### 48.2 Binary Plist Body Handler

- [ ] **48.2.1** Implement binary plist body parsing and generation

**File:** `src/receiver/ap2/body_handler.rs`

```rust
//! Request/Response body handling for AirPlay 2
//!
//! AirPlay 2 uses binary plist (bplist00) format for most request and
//! response bodies. This module provides parsing and generation utilities.

use crate::protocol::plist::{BinaryPlistDecoder, BinaryPlistEncoder, PlistValue};
use std::collections::HashMap;

/// Content types used in AirPlay 2
pub mod content_types {
    pub const BINARY_PLIST: &str = "application/x-apple-binary-plist";
    pub const OCTET_STREAM: &str = "application/octet-stream";
    pub const TEXT_PARAMETERS: &str = "text/parameters";
    pub const SDP: &str = "application/sdp";
}

/// Parse a binary plist request body
pub fn parse_bplist_body(body: &[u8]) -> Result<PlistValue, BodyParseError> {
    if body.is_empty() {
        return Ok(PlistValue::Dict(HashMap::new()));
    }

    // Check magic header
    if body.len() < 8 || &body[..6] != b"bplist" {
        return Err(BodyParseError::InvalidMagic);
    }

    BinaryPlistDecoder::decode(body)
        .map_err(|e| BodyParseError::DecodeError(e.to_string()))
}

/// Encode a plist value to binary plist bytes
pub fn encode_bplist_body(value: &PlistValue) -> Result<Vec<u8>, BodyParseError> {
    BinaryPlistEncoder::encode(value)
        .map_err(|e| BodyParseError::EncodeError(e.to_string()))
}

/// Parse text/parameters body (key: value format)
pub fn parse_text_parameters(body: &[u8]) -> Result<HashMap<String, String>, BodyParseError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| BodyParseError::InvalidUtf8)?;

    let mut params = HashMap::new();

    for line in text.lines() {
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            params.insert(key, value);
        }
    }

    Ok(params)
}

/// Generate text/parameters body
pub fn encode_text_parameters(params: &HashMap<String, String>) -> Vec<u8> {
    let mut output = String::new();
    for (key, value) in params {
        output.push_str(&format!("{}: {}\r\n", key, value));
    }
    output.into_bytes()
}

/// Helper to extract typed values from plist dictionaries
pub trait PlistExt {
    fn get_string(&self, key: &str) -> Option<&str>;
    fn get_int(&self, key: &str) -> Option<i64>;
    fn get_bytes(&self, key: &str) -> Option<&[u8]>;
    fn get_bool(&self, key: &str) -> Option<bool>;
    fn get_dict(&self, key: &str) -> Option<&HashMap<String, PlistValue>>;
    fn get_array(&self, key: &str) -> Option<&Vec<PlistValue>>;
}

impl PlistExt for PlistValue {
    fn get_string(&self, key: &str) -> Option<&str> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::String(s)) = dict.get(key) {
                return Some(s.as_str());
            }
        }
        None
    }

    fn get_int(&self, key: &str) -> Option<i64> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::Integer(i)) = dict.get(key) {
                return Some(*i);
            }
        }
        None
    }

    fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::Data(data)) = dict.get(key) {
                return Some(data.as_slice());
            }
        }
        None
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::Boolean(b)) = dict.get(key) {
                return Some(*b);
            }
        }
        None
    }

    fn get_dict(&self, key: &str) -> Option<&HashMap<String, PlistValue>> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::Dict(d)) = dict.get(key) {
                return Some(d);
            }
        }
        None
    }

    fn get_array(&self, key: &str) -> Option<&Vec<PlistValue>> {
        if let PlistValue::Dict(dict) = self {
            if let Some(PlistValue::Array(a)) = dict.get(key) {
                return Some(a);
            }
        }
        None
    }
}

/// Builder for plist response bodies
#[derive(Debug, Default)]
pub struct PlistResponseBuilder {
    values: HashMap<String, PlistValue>,
}

impl PlistResponseBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn string(mut self, key: &str, value: impl Into<String>) -> Self {
        self.values.insert(key.to_string(), PlistValue::String(value.into()));
        self
    }

    pub fn int(mut self, key: &str, value: i64) -> Self {
        self.values.insert(key.to_string(), PlistValue::Integer(value));
        self
    }

    pub fn bool(mut self, key: &str, value: bool) -> Self {
        self.values.insert(key.to_string(), PlistValue::Boolean(value));
        self
    }

    pub fn data(mut self, key: &str, value: Vec<u8>) -> Self {
        self.values.insert(key.to_string(), PlistValue::Data(value));
        self
    }

    pub fn dict(mut self, key: &str, value: HashMap<String, PlistValue>) -> Self {
        self.values.insert(key.to_string(), PlistValue::Dict(value));
        self
    }

    pub fn build(self) -> PlistValue {
        PlistValue::Dict(self.values)
    }

    pub fn encode(self) -> Result<Vec<u8>, BodyParseError> {
        encode_bplist_body(&self.build())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BodyParseError {
    #[error("Invalid binary plist magic header")]
    InvalidMagic,

    #[error("Failed to decode plist: {0}")]
    DecodeError(String),

    #[error("Failed to encode plist: {0}")]
    EncodeError(String),

    #[error("Invalid UTF-8 in text body")]
    InvalidUtf8,

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field type for {0}: expected {1}")]
    InvalidType(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_parameters_roundtrip() {
        let mut params = HashMap::new();
        params.insert("volume".to_string(), "-15.0".to_string());
        params.insert("progress".to_string(), "0/44100/88200".to_string());

        let encoded = encode_text_parameters(&params);
        let decoded = parse_text_parameters(&encoded).unwrap();

        assert_eq!(decoded.get("volume"), Some(&"-15.0".to_string()));
        assert_eq!(decoded.get("progress"), Some(&"0/44100/88200".to_string()));
    }

    #[test]
    fn test_plist_builder() {
        let plist = PlistResponseBuilder::new()
            .string("name", "Test Device")
            .int("port", 7000)
            .bool("enabled", true)
            .build();

        assert_eq!(plist.get_string("name"), Some("Test Device"));
        assert_eq!(plist.get_int("port"), Some(7000));
        assert_eq!(plist.get_bool("enabled"), Some(true));
    }
}
```

---

### 48.3 Extended Response Builder

- [ ] **48.3.1** Extend response builder for AirPlay 2

**File:** `src/receiver/ap2/response_builder.rs`

```rust
//! Extended RTSP response builder for AirPlay 2
//!
//! Adds support for binary plist bodies and AirPlay 2-specific headers.

use crate::protocol::rtsp::server_codec::ResponseBuilder;
use crate::protocol::rtsp::StatusCode;
use crate::protocol::plist::PlistValue;
use super::body_handler::{encode_bplist_body, content_types};

/// Extended response builder for AirPlay 2
pub struct Ap2ResponseBuilder {
    inner: ResponseBuilder,
}

impl Ap2ResponseBuilder {
    /// Create a new OK response
    pub fn ok() -> Self {
        Self {
            inner: ResponseBuilder::ok(),
        }
    }

    /// Create an error response
    pub fn error(status: StatusCode) -> Self {
        Self {
            inner: ResponseBuilder::error(status),
        }
    }

    /// Set the CSeq header
    pub fn cseq(mut self, cseq: u32) -> Self {
        self.inner = self.inner.cseq(cseq);
        self
    }

    /// Set the Session header
    pub fn session(mut self, session_id: &str) -> Self {
        self.inner = self.inner.session(session_id);
        self
    }

    /// Add a custom header
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.inner = self.inner.header(name, value);
        self
    }

    /// Set binary plist body
    pub fn bplist_body(self, value: &PlistValue) -> Result<Self, Ap2ResponseError> {
        let body = encode_bplist_body(value)
            .map_err(Ap2ResponseError::EncodeError)?;

        Ok(Self {
            inner: self.inner.binary_body(body, content_types::BINARY_PLIST),
        })
    }

    /// Set raw binary body with octet-stream content type
    pub fn binary_body(self, body: Vec<u8>) -> Self {
        Self {
            inner: self.inner.binary_body(body, content_types::OCTET_STREAM),
        }
    }

    /// Set text/parameters body
    pub fn text_body(mut self, body: &str) -> Self {
        self.inner = self.inner.text_body(body);
        self
    }

    /// Add Server header (common for AirPlay 2)
    pub fn server(self, version: &str) -> Self {
        self.header("Server", &format!("AirTunes/{}", version))
    }

    /// Add timing headers for SETUP response
    pub fn timing_port(self, port: u16) -> Self {
        self.header("Timing-Port", &port.to_string())
    }

    /// Add event port header for SETUP response
    pub fn event_port(self, port: u16) -> Self {
        self.header("Event-Port", &port.to_string())
    }

    /// Encode to bytes
    pub fn encode(self) -> Vec<u8> {
        self.inner.encode()
    }
}

/// Common response helpers
impl Ap2ResponseBuilder {
    /// Create response for successful pairing step
    pub fn pairing_response(cseq: u32, body: Vec<u8>) -> Self {
        Self::ok()
            .cseq(cseq)
            .binary_body(body)
    }

    /// Create response for authentication required
    pub fn auth_required(cseq: u32) -> Self {
        Self::error(StatusCode(470))  // Connection Authorization Required
            .cseq(cseq)
    }

    /// Create response for bad request with error dict
    pub fn bad_request_with_error(cseq: u32, code: i64, message: &str) -> Result<Self, Ap2ResponseError> {
        use std::collections::HashMap;

        let mut error_dict = HashMap::new();
        error_dict.insert("code".to_string(), PlistValue::Integer(code));
        error_dict.insert("message".to_string(), PlistValue::String(message.to_string()));

        let plist = PlistValue::Dict(error_dict);

        Self::error(StatusCode::BAD_REQUEST)
            .cseq(cseq)
            .bplist_body(&plist)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Ap2ResponseError {
    #[error("Failed to encode body: {0}")]
    EncodeError(#[from] super::body_handler::BodyParseError),
}

/// Additional status codes for AirPlay 2
impl StatusCode {
    /// 470 - Connection Authorization Required (pairing needed)
    pub const CONNECTION_AUTH_REQUIRED: StatusCode = StatusCode(470);

    /// 471 - Connection Credentials Required
    pub const CONNECTION_CREDENTIALS_REQUIRED: StatusCode = StatusCode(471);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_bplist_response() {
        let mut dict = HashMap::new();
        dict.insert("status".to_string(), PlistValue::Integer(0));
        dict.insert("message".to_string(), PlistValue::String("OK".to_string()));

        let plist = PlistValue::Dict(dict);

        let response = Ap2ResponseBuilder::ok()
            .cseq(5)
            .bplist_body(&plist)
            .unwrap()
            .encode();

        let response_str = String::from_utf8_lossy(&response);
        assert!(response_str.contains("200 OK"));
        assert!(response_str.contains("Content-Type: application/x-apple-binary-plist"));
    }

    #[test]
    fn test_timing_response() {
        let response = Ap2ResponseBuilder::ok()
            .cseq(10)
            .timing_port(7011)
            .event_port(7012)
            .encode();

        let response_str = String::from_utf8_lossy(&response);
        assert!(response_str.contains("Timing-Port: 7011"));
        assert!(response_str.contains("Event-Port: 7012"));
    }
}
```

---

### 48.4 Request Handler Framework

- [ ] **48.4.1** Implement unified request handler for AirPlay 2

**File:** `src/receiver/ap2/request_handler.rs`

```rust
//! Unified request handler for AirPlay 2 receiver
//!
//! Routes requests to appropriate handlers based on classification,
//! manages session state, and handles encryption/decryption.

use crate::protocol::rtsp::RtspRequest;
use super::request_router::{Ap2RequestType, Ap2Endpoint, RtspMethod};
use super::session_state::Ap2SessionState;
use super::response_builder::Ap2ResponseBuilder;
use super::body_handler::parse_bplist_body;
use crate::protocol::rtsp::StatusCode;

/// Result of handling a request
#[derive(Debug)]
pub struct Ap2HandleResult {
    /// Response bytes to send
    pub response: Vec<u8>,

    /// New session state (if changed)
    pub new_state: Option<Ap2SessionState>,

    /// Event to emit (for audio pipeline control)
    pub event: Option<Ap2Event>,

    /// Error that occurred (for logging)
    pub error: Option<String>,
}

/// Events emitted by request handling
#[derive(Debug, Clone)]
pub enum Ap2Event {
    /// Pairing completed, session keys available
    PairingComplete {
        session_key: Vec<u8>,
    },

    /// First SETUP phase complete, timing/event channels ready
    SetupPhase1Complete {
        timing_port: u16,
        event_port: u16,
    },

    /// Second SETUP phase complete, audio channels ready
    SetupPhase2Complete {
        audio_data_port: u16,
        audio_control_port: u16,
    },

    /// Streaming started
    StreamingStarted {
        initial_sequence: u16,
        initial_timestamp: u32,
    },

    /// Streaming paused
    StreamingPaused,

    /// Buffer flush requested
    FlushRequested {
        until_sequence: Option<u16>,
        until_timestamp: Option<u32>,
    },

    /// Session teardown
    Teardown,

    /// Volume changed
    VolumeChanged {
        volume: f32,
    },

    /// Metadata updated
    MetadataUpdated,

    /// Command received
    CommandReceived {
        command: String,
    },
}

/// Context for request handling
pub struct Ap2RequestContext<'a> {
    /// Current session state
    pub state: &'a Ap2SessionState,

    /// Session ID (if established)
    pub session_id: Option<&'a str>,

    /// Encryption enabled
    pub encrypted: bool,

    /// Decryption function (if encrypted)
    pub decrypt: Option<&'a dyn Fn(&[u8]) -> Result<Vec<u8>, String>>,
}

/// Handle an AirPlay 2 request
pub fn handle_ap2_request(
    request: &RtspRequest,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    let cseq = request.headers.cseq().unwrap_or(0);
    let request_type = Ap2RequestType::classify(request);

    // Check if request is allowed in current state
    let method_name = match &request_type {
        Ap2RequestType::Rtsp(m) => format!("{:?}", m),
        Ap2RequestType::Endpoint(e) => format!("{:?}", e),
        Ap2RequestType::Unknown => "UNKNOWN".to_string(),
    };

    if !context.state.allows_method(&method_name) {
        return Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::METHOD_NOT_VALID)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(format!("Method {} not allowed in state {:?}", method_name, context.state)),
        };
    }

    // Check authentication requirements
    if let Ap2RequestType::Endpoint(ref endpoint) = request_type {
        if endpoint.requires_auth() && !context.state.is_authenticated() {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::auth_required(cseq).encode(),
                new_state: None,
                event: None,
                error: Some("Authentication required".to_string()),
            };
        }
    }

    // Route to appropriate handler
    match request_type {
        Ap2RequestType::Rtsp(method) => handle_rtsp_method(method, request, cseq, context, handlers),
        Ap2RequestType::Endpoint(endpoint) => handle_endpoint(endpoint, request, cseq, context, handlers),
        Ap2RequestType::Unknown => Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some("Unknown request type".to_string()),
        },
    }
}

fn handle_rtsp_method(
    method: RtspMethod,
    request: &RtspRequest,
    cseq: u32,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    match method {
        RtspMethod::Options => handle_options(cseq),
        RtspMethod::Setup => (handlers.setup)(request, cseq, context),
        RtspMethod::Record => (handlers.record)(request, cseq, context),
        RtspMethod::Pause => (handlers.pause)(request, cseq, context),
        RtspMethod::Flush => (handlers.flush)(request, cseq, context),
        RtspMethod::Teardown => (handlers.teardown)(request, cseq, context),
        RtspMethod::GetParameter => (handlers.get_parameter)(request, cseq, context),
        RtspMethod::SetParameter => (handlers.set_parameter)(request, cseq, context),
        RtspMethod::Get => {
            // GET /info is handled as endpoint
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_FOUND)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: None,
            }
        }
    }
}

fn handle_endpoint(
    endpoint: Ap2Endpoint,
    request: &RtspRequest,
    cseq: u32,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    match endpoint {
        Ap2Endpoint::Info => (handlers.info)(request, cseq, context),
        Ap2Endpoint::PairSetup => (handlers.pair_setup)(request, cseq, context),
        Ap2Endpoint::PairVerify => (handlers.pair_verify)(request, cseq, context),
        Ap2Endpoint::Command => (handlers.command)(request, cseq, context),
        Ap2Endpoint::Feedback => (handlers.feedback)(request, cseq, context),
        Ap2Endpoint::AudioMode => (handlers.audio_mode)(request, cseq, context),
        Ap2Endpoint::AuthSetup => (handlers.auth_setup)(request, cseq, context),

        Ap2Endpoint::FairPlaySetup => {
            // FairPlay not supported
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some("FairPlay not supported".to_string()),
            }
        }

        Ap2Endpoint::Unknown(path) => {
            log::warn!("Unknown endpoint: {}", path);
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_FOUND)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some(format!("Unknown endpoint: {}", path)),
            }
        }
    }
}

fn handle_options(cseq: u32) -> Ap2HandleResult {
    let methods = [
        "OPTIONS", "GET", "POST", "SETUP", "RECORD", "PAUSE",
        "FLUSH", "TEARDOWN", "GET_PARAMETER", "SET_PARAMETER"
    ].join(", ");

    Ap2HandleResult {
        response: Ap2ResponseBuilder::ok()
            .cseq(cseq)
            .header("Public", &methods)
            .server("366.0")
            .encode(),
        new_state: None,
        event: None,
        error: None,
    }
}

/// Handler function type
pub type HandlerFn = fn(&RtspRequest, u32, &Ap2RequestContext) -> Ap2HandleResult;

/// Collection of request handlers
pub struct Ap2Handlers {
    pub info: HandlerFn,
    pub pair_setup: HandlerFn,
    pub pair_verify: HandlerFn,
    pub auth_setup: HandlerFn,
    pub setup: HandlerFn,
    pub record: HandlerFn,
    pub pause: HandlerFn,
    pub flush: HandlerFn,
    pub teardown: HandlerFn,
    pub get_parameter: HandlerFn,
    pub set_parameter: HandlerFn,
    pub command: HandlerFn,
    pub feedback: HandlerFn,
    pub audio_mode: HandlerFn,
}

impl Default for Ap2Handlers {
    fn default() -> Self {
        Self {
            info: stub_handler,
            pair_setup: stub_handler,
            pair_verify: stub_handler,
            auth_setup: stub_handler,
            setup: stub_handler,
            record: stub_handler,
            pause: stub_handler,
            flush: stub_handler,
            teardown: stub_handler,
            get_parameter: stub_handler,
            set_parameter: stub_handler,
            command: stub_handler,
            feedback: stub_handler,
            audio_mode: stub_handler,
        }
    }
}

/// Stub handler for unimplemented endpoints
fn stub_handler(_request: &RtspRequest, cseq: u32, _context: &Ap2RequestContext) -> Ap2HandleResult {
    Ap2HandleResult {
        response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
            .cseq(cseq)
            .encode(),
        new_state: None,
        event: None,
        error: Some("Handler not implemented".to_string()),
    }
}
```

---

## Unit Tests

### 48.5 Server Extension Tests

- [ ] **48.5.1** Comprehensive tests for request routing and handling

**File:** `src/receiver/ap2/request_handler.rs` (test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::rtsp::{RtspRequest, Method, Headers};

    fn make_request(method: Method, uri: &str) -> RtspRequest {
        let mut headers = Headers::new();
        headers.insert("CSeq".to_string(), "1".to_string());

        RtspRequest {
            method,
            uri: uri.to_string(),
            headers,
            body: vec![],
        }
    }

    fn make_context() -> Ap2RequestContext<'static> {
        Ap2RequestContext {
            state: &Ap2SessionState::Connected,
            session_id: None,
            encrypted: false,
            decrypt: None,
        }
    }

    #[test]
    fn test_options_always_allowed() {
        let request = make_request(Method::Options, "*");
        let context = make_context();
        let handlers = Ap2Handlers::default();

        let result = handle_ap2_request(&request, &context, &handlers);

        let response_str = String::from_utf8_lossy(&result.response);
        assert!(response_str.contains("200 OK"));
        assert!(response_str.contains("Public:"));
    }

    #[test]
    fn test_unauthenticated_command_rejected() {
        let request = make_request(Method::Post, "/command");
        let context = make_context();
        let handlers = Ap2Handlers::default();

        let result = handle_ap2_request(&request, &context, &handlers);

        let response_str = String::from_utf8_lossy(&result.response);
        assert!(response_str.contains("470"));  // Auth required
    }

    #[test]
    fn test_pair_setup_allowed_unauthenticated() {
        let request = make_request(Method::Post, "/pair-setup");

        // Use a handler that returns OK
        let mut handlers = Ap2Handlers::default();
        handlers.pair_setup = |_, cseq, _| Ap2HandleResult {
            response: Ap2ResponseBuilder::ok().cseq(cseq).encode(),
            new_state: None,
            event: None,
            error: None,
        };

        let context = make_context();
        let result = handle_ap2_request(&request, &context, &handlers);

        let response_str = String::from_utf8_lossy(&result.response);
        assert!(response_str.contains("200 OK"));
    }

    #[test]
    fn test_setup_requires_paired_state() {
        let request = make_request(Method::Setup, "rtsp://192.168.1.1/12345");

        let context = Ap2RequestContext {
            state: &Ap2SessionState::Connected,
            session_id: None,
            encrypted: false,
            decrypt: None,
        };

        let handlers = Ap2Handlers::default();
        let result = handle_ap2_request(&request, &context, &handlers);

        let response_str = String::from_utf8_lossy(&result.response);
        // Should be rejected - not in paired state
        assert!(response_str.contains("455") || response_str.contains("Not Valid"));
    }
}
```

---

## Acceptance Criteria

- [ ] Request classification correctly identifies RTSP vs endpoint requests
- [ ] Binary plist bodies parse and encode correctly
- [ ] Response builder generates valid RTSP responses with bplist bodies
- [ ] Request routing enforces state-based access control
- [ ] Authentication requirements enforced for protected endpoints
- [ ] OPTIONS returns correct Public header for AirPlay 2
- [ ] Unknown endpoints return 404
- [ ] All unit tests pass

---

## Notes

### Encrypted Request Handling

After pairing completes, all control channel traffic is encrypted using ChaCha20-Poly1305.
The request handler framework supports this via the `decrypt` function in the context, but
the actual encryption/decryption is handled by Section 53 (Encrypted Control Channel).

### Request Body Processing

Most AirPlay 2 endpoints expect binary plist bodies. The handler framework parses these
automatically when the Content-Type header indicates bplist. Text parameters are still
supported for backward compatibility with some SET_PARAMETER commands.

---

## References

- [AirPlay 2 Protocol Analysis](https://emanuelecozzi.net/docs/airplay2)
- [Section 36: RTSP Server Sans-IO](./complete/36-rtsp-server-sans-io.md)
- [Section 03: Binary Plist Codec](./complete/03-binary-plist-codec.md)
