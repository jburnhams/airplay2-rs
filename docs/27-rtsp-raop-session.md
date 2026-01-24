# Section 27: RTSP Session for RAOP

## Dependencies
- **Section 05**: RTSP Protocol (must be complete)
- **Section 26**: RSA Authentication (must be complete)
- **Section 25**: RAOP Discovery (recommended)

## Overview

AirPlay 1 uses RTSP (Real Time Streaming Protocol) for session control, similar to AirPlay 2 but with significant differences:

- **SDP bodies** instead of binary plist for stream configuration
- **Apple-Challenge/Response headers** for device authentication
- **Different method sequence** (OPTIONS → ANNOUNCE → SETUP → RECORD)
- **Legacy headers** (CSeq, Session, RTP-Info)

This section extends the existing RTSP codec to support RAOP-specific requirements.

## Objectives

- Implement SDP (Session Description Protocol) parsing and generation
- Add RAOP-specific RTSP headers
- Implement RAOP session state machine
- Support all RAOP RTSP methods
- Handle audio format negotiation via SDP

---

## Tasks

### 27.1 SDP Protocol Implementation

- [ ] **27.1.1** Define SDP types and parser

**File:** `src/protocol/sdp/mod.rs`

```rust
//! SDP (Session Description Protocol) for RAOP
//!
//! RAOP uses SDP in the ANNOUNCE request to describe the audio stream.

mod parser;
mod builder;

pub use parser::{SdpParser, SdpParseError};
pub use builder::SdpBuilder;

use std::collections::HashMap;

/// SDP session description
#[derive(Debug, Clone, Default)]
pub struct SessionDescription {
    /// Protocol version (v=)
    pub version: u8,
    /// Origin (o=)
    pub origin: Option<SdpOrigin>,
    /// Session name (s=)
    pub session_name: String,
    /// Connection info (c=)
    pub connection: Option<SdpConnection>,
    /// Timing (t=)
    pub timing: Option<(u64, u64)>,
    /// Media descriptions (m=)
    pub media: Vec<MediaDescription>,
    /// Session-level attributes (a=)
    pub attributes: HashMap<String, Option<String>>,
}

/// SDP origin field (o=)
#[derive(Debug, Clone)]
pub struct SdpOrigin {
    /// Username
    pub username: String,
    /// Session ID
    pub session_id: String,
    /// Session version
    pub session_version: String,
    /// Network type (usually "IN")
    pub net_type: String,
    /// Address type (usually "IP4" or "IP6")
    pub addr_type: String,
    /// Unicast address
    pub unicast_address: String,
}

/// SDP connection field (c=)
#[derive(Debug, Clone)]
pub struct SdpConnection {
    /// Network type
    pub net_type: String,
    /// Address type
    pub addr_type: String,
    /// Connection address
    pub address: String,
}

/// SDP media description (m=)
#[derive(Debug, Clone)]
pub struct MediaDescription {
    /// Media type (audio, video, etc.)
    pub media_type: String,
    /// Port number
    pub port: u16,
    /// Protocol (RTP/AVP, etc.)
    pub protocol: String,
    /// Format list (payload types)
    pub formats: Vec<String>,
    /// Media-level attributes
    pub attributes: HashMap<String, Option<String>>,
}

impl SessionDescription {
    /// Get a session-level attribute
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name)?.as_deref()
    }

    /// Get the audio media description
    pub fn audio_media(&self) -> Option<&MediaDescription> {
        self.media.iter().find(|m| m.media_type == "audio")
    }

    /// Get the rsaaeskey attribute (encrypted AES key)
    pub fn rsaaeskey(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("rsaaeskey")?
            .as_deref()
            .or_else(|| self.get_attribute("rsaaeskey"))
    }

    /// Get the aesiv attribute (AES initialization vector)
    pub fn aesiv(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("aesiv")?
            .as_deref()
            .or_else(|| self.get_attribute("aesiv"))
    }

    /// Get the fmtp attribute (format parameters)
    pub fn fmtp(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("fmtp")?
            .as_deref()
    }

    /// Get the rtpmap attribute
    pub fn rtpmap(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("rtpmap")?
            .as_deref()
    }
}

impl MediaDescription {
    /// Get a media-level attribute
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name)?.as_deref()
    }
}
```

- [ ] **27.1.2** Implement SDP parser

**File:** `src/protocol/sdp/parser.rs`

```rust
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdpParseError {
    #[error("invalid version line")]
    InvalidVersion,
    #[error("invalid origin line: {0}")]
    InvalidOrigin(String),
    #[error("invalid connection line: {0}")]
    InvalidConnection(String),
    #[error("invalid media line: {0}")]
    InvalidMedia(String),
    #[error("invalid attribute: {0}")]
    InvalidAttribute(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

/// SDP parser
pub struct SdpParser;

impl SdpParser {
    /// Parse SDP from string
    pub fn parse(input: &str) -> Result<SessionDescription, SdpParseError> {
        let mut sdp = SessionDescription::default();
        let mut current_media: Option<MediaDescription> = None;

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.len() < 2 || line.chars().nth(1) != Some('=') {
                continue;
            }

            let type_char = line.chars().next().unwrap();
            let value = &line[2..];

            match type_char {
                'v' => {
                    sdp.version = value.parse().map_err(|_| SdpParseError::InvalidVersion)?;
                }
                'o' => {
                    sdp.origin = Some(Self::parse_origin(value)?);
                }
                's' => {
                    sdp.session_name = value.to_string();
                }
                'c' => {
                    let conn = Self::parse_connection(value)?;
                    if current_media.is_some() {
                        // Connection for current media
                    } else {
                        sdp.connection = Some(conn);
                    }
                }
                't' => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        sdp.timing = Some((
                            parts[0].parse().unwrap_or(0),
                            parts[1].parse().unwrap_or(0),
                        ));
                    }
                }
                'm' => {
                    // Save previous media if any
                    if let Some(media) = current_media.take() {
                        sdp.media.push(media);
                    }
                    current_media = Some(Self::parse_media(value)?);
                }
                'a' => {
                    let (name, value) = Self::parse_attribute(value)?;
                    if let Some(ref mut media) = current_media {
                        media.attributes.insert(name, value);
                    } else {
                        sdp.attributes.insert(name, value);
                    }
                }
                _ => {
                    // Ignore unknown lines
                }
            }
        }

        // Save last media
        if let Some(media) = current_media {
            sdp.media.push(media);
        }

        Ok(sdp)
    }

    fn parse_origin(value: &str) -> Result<SdpOrigin, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 6 {
            return Err(SdpParseError::InvalidOrigin(value.to_string()));
        }

        Ok(SdpOrigin {
            username: parts[0].to_string(),
            session_id: parts[1].to_string(),
            session_version: parts[2].to_string(),
            net_type: parts[3].to_string(),
            addr_type: parts[4].to_string(),
            unicast_address: parts[5].to_string(),
        })
    }

    fn parse_connection(value: &str) -> Result<SdpConnection, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(SdpParseError::InvalidConnection(value.to_string()));
        }

        Ok(SdpConnection {
            net_type: parts[0].to_string(),
            addr_type: parts[1].to_string(),
            address: parts[2].to_string(),
        })
    }

    fn parse_media(value: &str) -> Result<MediaDescription, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(SdpParseError::InvalidMedia(value.to_string()));
        }

        Ok(MediaDescription {
            media_type: parts[0].to_string(),
            port: parts[1].parse().unwrap_or(0),
            protocol: parts[2].to_string(),
            formats: parts[3..].iter().map(|s| s.to_string()).collect(),
            attributes: HashMap::new(),
        })
    }

    fn parse_attribute(value: &str) -> Result<(String, Option<String>), SdpParseError> {
        if let Some(colon_pos) = value.find(':') {
            let name = value[..colon_pos].to_string();
            let attr_value = value[colon_pos + 1..].to_string();
            Ok((name, Some(attr_value)))
        } else {
            Ok((value.to_string(), None))
        }
    }
}
```

- [ ] **27.1.3** Implement SDP builder

**File:** `src/protocol/sdp/builder.rs`

```rust
use super::*;

/// Builder for SDP session descriptions
pub struct SdpBuilder {
    sdp: SessionDescription,
    current_media: Option<MediaDescription>,
}

impl SdpBuilder {
    /// Create a new SDP builder
    pub fn new() -> Self {
        Self {
            sdp: SessionDescription {
                version: 0,
                ..Default::default()
            },
            current_media: None,
        }
    }

    /// Set origin
    pub fn origin(
        mut self,
        username: &str,
        session_id: &str,
        addr: &str,
    ) -> Self {
        self.sdp.origin = Some(SdpOrigin {
            username: username.to_string(),
            session_id: session_id.to_string(),
            session_version: "1".to_string(),
            net_type: "IN".to_string(),
            addr_type: if addr.contains(':') { "IP6" } else { "IP4" }.to_string(),
            unicast_address: addr.to_string(),
        });
        self
    }

    /// Set session name
    pub fn session_name(mut self, name: &str) -> Self {
        self.sdp.session_name = name.to_string();
        self
    }

    /// Set connection info
    pub fn connection(mut self, addr: &str) -> Self {
        self.sdp.connection = Some(SdpConnection {
            net_type: "IN".to_string(),
            addr_type: if addr.contains(':') { "IP6" } else { "IP4" }.to_string(),
            address: addr.to_string(),
        });
        self
    }

    /// Set timing (usually 0 0 for live streams)
    pub fn timing(mut self, start: u64, stop: u64) -> Self {
        self.sdp.timing = Some((start, stop));
        self
    }

    /// Add session-level attribute
    pub fn attribute(mut self, name: &str, value: Option<&str>) -> Self {
        self.sdp.attributes.insert(
            name.to_string(),
            value.map(String::from),
        );
        self
    }

    /// Start a media section
    pub fn media(
        mut self,
        media_type: &str,
        port: u16,
        protocol: &str,
        formats: &[&str],
    ) -> Self {
        // Save previous media if any
        if let Some(media) = self.current_media.take() {
            self.sdp.media.push(media);
        }

        self.current_media = Some(MediaDescription {
            media_type: media_type.to_string(),
            port,
            protocol: protocol.to_string(),
            formats: formats.iter().map(|s| s.to_string()).collect(),
            attributes: HashMap::new(),
        });

        self
    }

    /// Add media-level attribute
    pub fn media_attribute(mut self, name: &str, value: Option<&str>) -> Self {
        if let Some(ref mut media) = self.current_media {
            media.attributes.insert(
                name.to_string(),
                value.map(String::from),
            );
        }
        self
    }

    /// Build the SDP
    pub fn build(mut self) -> SessionDescription {
        // Save last media
        if let Some(media) = self.current_media.take() {
            self.sdp.media.push(media);
        }
        self.sdp
    }

    /// Build and encode as string
    pub fn encode(self) -> String {
        let sdp = self.build();
        encode_sdp(&sdp)
    }
}

/// Encode SDP to string format
pub fn encode_sdp(sdp: &SessionDescription) -> String {
    let mut output = String::new();

    // Version
    output.push_str(&format!("v={}\r\n", sdp.version));

    // Origin
    if let Some(ref o) = sdp.origin {
        output.push_str(&format!(
            "o={} {} {} {} {} {}\r\n",
            o.username, o.session_id, o.session_version,
            o.net_type, o.addr_type, o.unicast_address
        ));
    }

    // Session name
    output.push_str(&format!("s={}\r\n", sdp.session_name));

    // Connection
    if let Some(ref c) = sdp.connection {
        output.push_str(&format!(
            "c={} {} {}\r\n",
            c.net_type, c.addr_type, c.address
        ));
    }

    // Timing
    if let Some((start, stop)) = sdp.timing {
        output.push_str(&format!("t={} {}\r\n", start, stop));
    }

    // Session attributes
    for (name, value) in &sdp.attributes {
        if let Some(v) = value {
            output.push_str(&format!("a={}:{}\r\n", name, v));
        } else {
            output.push_str(&format!("a={}\r\n", name));
        }
    }

    // Media sections
    for media in &sdp.media {
        output.push_str(&format!(
            "m={} {} {} {}\r\n",
            media.media_type,
            media.port,
            media.protocol,
            media.formats.join(" ")
        ));

        for (name, value) in &media.attributes {
            if let Some(v) = value {
                output.push_str(&format!("a={}:{}\r\n", name, v));
            } else {
                output.push_str(&format!("a={}\r\n", name));
            }
        }
    }

    output
}

/// Create RAOP ANNOUNCE SDP for Apple Lossless audio
pub fn create_raop_announce_sdp(
    session_id: &str,
    client_ip: &str,
    server_ip: &str,
    rsaaeskey: &str,
    aesiv: &str,
) -> String {
    SdpBuilder::new()
        .origin("iTunes", session_id, client_ip)
        .session_name("iTunes")
        .connection(server_ip)
        .timing(0, 0)
        .media("audio", 0, "RTP/AVP", &["96"])
        .media_attribute("rtpmap", Some("96 AppleLossless"))
        .media_attribute("fmtp", Some("96 352 0 16 40 10 14 2 255 0 0 44100"))
        .media_attribute("rsaaeskey", Some(rsaaeskey))
        .media_attribute("aesiv", Some(aesiv))
        .media_attribute("min-latency", Some("11025"))
        .encode()
}
```

---

### 27.2 RAOP RTSP Extensions

- [ ] **27.2.1** Add RAOP-specific headers

**File:** `src/protocol/rtsp/headers.rs` (additions)

```rust
// Add to existing headers module:

/// RAOP-specific header names
pub mod raop {
    /// Apple challenge for authentication
    pub const APPLE_CHALLENGE: &str = "Apple-Challenge";
    /// Apple response to challenge
    pub const APPLE_RESPONSE: &str = "Apple-Response";
    /// Audio latency in samples
    pub const AUDIO_LATENCY: &str = "Audio-Latency";
    /// Audio jack status
    pub const AUDIO_JACK_STATUS: &str = "Audio-Jack-Status";
    /// Client instance ID
    pub const CLIENT_INSTANCE: &str = "Client-Instance";
    /// DACP ID for remote control
    pub const DACP_ID: &str = "DACP-ID";
    /// Active remote token
    pub const ACTIVE_REMOTE: &str = "Active-Remote";
    /// Server info header
    pub const SERVER: &str = "Server";
    /// Range header for RECORD
    pub const RANGE: &str = "Range";
}
```

- [ ] **27.2.2** Implement RAOP RTSP session

**File:** `src/protocol/raop/session.rs`

```rust
//! RAOP RTSP session management

use crate::protocol::rtsp::{
    Method, RtspRequest, RtspRequestBuilder, RtspResponse,
    Headers, headers::names, headers::raop,
};
use super::auth::RaopAuthenticator;
use super::key_exchange::RaopSessionKeys;

/// RAOP session states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaopSessionState {
    /// Initial state
    Init,
    /// OPTIONS sent, checking authentication
    OptionsExchange,
    /// ANNOUNCE sent with stream params
    Announcing,
    /// SETUP sent, configuring transport
    SettingUp,
    /// RECORD sent, streaming
    Recording,
    /// Paused (FLUSH sent)
    Paused,
    /// Session terminated
    Terminated,
}

/// Transport configuration from SETUP
#[derive(Debug, Clone)]
pub struct RaopTransport {
    /// Server audio data port
    pub server_port: u16,
    /// Server control port
    pub control_port: u16,
    /// Server timing port
    pub timing_port: u16,
    /// Client control port
    pub client_control_port: u16,
    /// Client timing port
    pub client_timing_port: u16,
}

/// RAOP RTSP session manager
pub struct RaopRtspSession {
    /// Current state
    state: RaopSessionState,
    /// CSeq counter
    cseq: u32,
    /// Server session ID
    session_id: Option<String>,
    /// Client instance ID (64-bit hex)
    client_instance: String,
    /// DACP ID for remote control
    dacp_id: String,
    /// Active remote token
    active_remote: String,
    /// Server address
    server_addr: String,
    /// Server port
    server_port: u16,
    /// Authentication state
    authenticator: RaopAuthenticator,
    /// Session encryption keys
    session_keys: Option<RaopSessionKeys>,
    /// Transport configuration
    transport: Option<RaopTransport>,
    /// Audio latency (samples)
    audio_latency: u32,
}

impl RaopRtspSession {
    /// Create a new RAOP session
    pub fn new(server_addr: &str, server_port: u16) -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        Self {
            state: RaopSessionState::Init,
            cseq: 0,
            session_id: None,
            client_instance: format!("{:016X}", rng.gen::<u64>()),
            dacp_id: format!("{:016X}", rng.gen::<u64>()),
            active_remote: rng.gen::<u32>().to_string(),
            server_addr: server_addr.to_string(),
            server_port,
            authenticator: RaopAuthenticator::new(),
            session_keys: None,
            transport: None,
            audio_latency: 11025, // Default ~250ms at 44.1kHz
        }
    }

    /// Get current state
    pub fn state(&self) -> RaopSessionState {
        self.state
    }

    /// Get transport configuration
    pub fn transport(&self) -> Option<&RaopTransport> {
        self.transport.as_ref()
    }

    /// Get session keys
    pub fn session_keys(&self) -> Option<&RaopSessionKeys> {
        self.session_keys.as_ref()
    }

    /// Get next CSeq
    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    /// Get base URI
    fn uri(&self, path: &str) -> String {
        if path.is_empty() {
            format!("rtsp://{}:{}/{}", self.server_addr, self.server_port, self.client_instance)
        } else {
            format!("rtsp://{}:{}/{}", self.server_addr, self.server_port, path)
        }
    }

    /// Add common headers to request
    fn add_common_headers(&self, builder: RtspRequestBuilder, cseq: u32) -> RtspRequestBuilder {
        let mut b = builder
            .cseq(cseq)
            .header(names::USER_AGENT, "iTunes/12.0 (Macintosh)")
            .header(raop::CLIENT_INSTANCE, &self.client_instance)
            .header(raop::DACP_ID, &self.dacp_id)
            .header(raop::ACTIVE_REMOTE, &self.active_remote);

        if let Some(ref session) = self.session_id {
            b = b.session(session);
        }

        b
    }

    /// Create OPTIONS request
    pub fn options_request(&mut self) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Options, self.uri("*"));

        self.add_common_headers(builder, cseq)
            .header(raop::APPLE_CHALLENGE, self.authenticator.challenge_header())
            .build()
    }

    /// Create ANNOUNCE request with SDP
    pub fn announce_request(&mut self, sdp: &str) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Announce, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "application/sdp")
            .body(sdp.as_bytes().to_vec())
            .build()
    }

    /// Create SETUP request
    pub fn setup_request(
        &mut self,
        control_port: u16,
        timing_port: u16,
    ) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Setup, self.uri(""));

        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={};timing_port={}",
            control_port, timing_port
        );

        self.add_common_headers(builder, cseq)
            .header(names::TRANSPORT, &transport)
            .build()
    }

    /// Create RECORD request
    pub fn record_request(&mut self, seq: u16, rtptime: u32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Record, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(raop::RANGE, "npt=0-")
            .header("RTP-Info", format!("seq={};rtptime={}", seq, rtptime))
            .build()
    }

    /// Create SET_PARAMETER request for volume
    pub fn set_volume_request(&mut self, volume_db: f32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        let body = format!("volume: {:.6}\r\n", volume_db);

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "text/parameters")
            .body(body.into_bytes())
            .build()
    }

    /// Create SET_PARAMETER request for progress
    pub fn set_progress_request(
        &mut self,
        start: u32,
        current: u32,
        end: u32,
    ) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        let body = format!("progress: {}/{}/{}\r\n", start, current, end);

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "text/parameters")
            .body(body.into_bytes())
            .build()
    }

    /// Create FLUSH request
    pub fn flush_request(&mut self, seq: u16, rtptime: u32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Flush, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header("RTP-Info", format!("seq={};rtptime={}", seq, rtptime))
            .build()
    }

    /// Create TEARDOWN request
    pub fn teardown_request(&mut self) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Teardown, self.uri(""));

        self.add_common_headers(builder, cseq).build()
    }

    /// Process response and update state
    pub fn process_response(
        &mut self,
        method: Method,
        response: &RtspResponse,
    ) -> Result<(), String> {
        if !response.is_success() {
            return Err(format!(
                "{} failed: {} {}",
                method.as_str(),
                response.status.as_u16(),
                response.reason
            ));
        }

        // Extract session ID
        if let Some(session) = response.session() {
            let session_id = session.split(';').next().unwrap_or(session);
            self.session_id = Some(session_id.to_string());
        }

        match method {
            Method::Options => {
                // Verify Apple-Response if present
                if let Some(apple_response) = response.headers.get(raop::APPLE_RESPONSE) {
                    // TODO: Verify with known server parameters
                    // For now, accept any response
                }
                self.authenticator.mark_sent();
                self.state = RaopSessionState::OptionsExchange;
            }
            Method::Announce => {
                self.state = RaopSessionState::Announcing;
            }
            Method::Setup => {
                // Parse transport response
                if let Some(transport) = response.headers.get(names::TRANSPORT) {
                    self.transport = Some(Self::parse_transport(transport)?);
                }
                // Extract audio latency
                if let Some(latency) = response.headers.get(raop::AUDIO_LATENCY) {
                    self.audio_latency = latency.parse().unwrap_or(11025);
                }
                self.state = RaopSessionState::SettingUp;
            }
            Method::Record => {
                self.state = RaopSessionState::Recording;
            }
            Method::Flush => {
                self.state = RaopSessionState::Paused;
            }
            Method::Teardown => {
                self.state = RaopSessionState::Terminated;
            }
            _ => {}
        }

        Ok(())
    }

    fn parse_transport(transport: &str) -> Result<RaopTransport, String> {
        // Parse transport header like:
        // RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002

        let mut server_port = 0u16;
        let mut control_port = 0u16;
        let mut timing_port = 0u16;

        for part in transport.split(';') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "server_port" => server_port = value.parse().unwrap_or(0),
                    "control_port" => control_port = value.parse().unwrap_or(0),
                    "timing_port" => timing_port = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if server_port == 0 {
            return Err("missing server_port in transport".to_string());
        }

        Ok(RaopTransport {
            server_port,
            control_port,
            timing_port,
            client_control_port: 0, // Set by caller
            client_timing_port: 0,
        })
    }

    /// Generate session keys and prepare ANNOUNCE
    pub fn prepare_announce(&mut self) -> Result<String, String> {
        let keys = RaopSessionKeys::generate()
            .map_err(|e| e.to_string())?;

        let sdp = crate::protocol::sdp::create_raop_announce_sdp(
            &self.client_instance,
            "0.0.0.0", // Will be filled by actual client IP
            &self.server_addr,
            &keys.rsaaeskey(),
            &keys.aesiv(),
        );

        self.session_keys = Some(keys);
        Ok(sdp)
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/sdp/parser.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_sdp() {
        let sdp_text = r#"v=0
o=iTunes 1234567890 1 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.50
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
"#;

        let sdp = SdpParser::parse(sdp_text).unwrap();

        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "iTunes");
        assert_eq!(sdp.media.len(), 1);

        let audio = sdp.audio_media().unwrap();
        assert_eq!(audio.media_type, "audio");
        assert_eq!(audio.protocol, "RTP/AVP");
    }

    #[test]
    fn test_parse_raop_announce() {
        let sdp_text = r#"v=0
o=iTunes 3413821438 1 IN IP4 fe80::217:f2ff:fe0f:e0f6
s=iTunes
c=IN IP4 fe80::5a55:caff:fe1a:e288
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:ABCDEF123456
a=aesiv:0011223344556677
a=min-latency:11025
"#;

        let sdp = SdpParser::parse(sdp_text).unwrap();

        assert_eq!(sdp.rsaaeskey(), Some("ABCDEF123456"));
        assert_eq!(sdp.aesiv(), Some("0011223344556677"));
        assert_eq!(sdp.fmtp(), Some("96 352 0 16 40 10 14 2 255 0 0 44100"));
    }

    #[test]
    fn test_parse_origin() {
        let sdp_text = "v=0\no=user 123 1 IN IP4 192.168.1.1\ns=test\n";
        let sdp = SdpParser::parse(sdp_text).unwrap();

        let origin = sdp.origin.unwrap();
        assert_eq!(origin.username, "user");
        assert_eq!(origin.session_id, "123");
        assert_eq!(origin.addr_type, "IP4");
    }
}
```

### Test File: `src/protocol/raop/session.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = RaopRtspSession::new("192.168.1.50", 5000);

        assert_eq!(session.state(), RaopSessionState::Init);
        assert!(session.session_id.is_none());
        assert!(!session.client_instance.is_empty());
    }

    #[test]
    fn test_options_request() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.options_request();

        assert_eq!(request.method, Method::Options);
        assert!(request.headers.get("Apple-Challenge").is_some());
        assert!(request.headers.get("CSeq").is_some());
        assert!(request.headers.get("Client-Instance").is_some());
    }

    #[test]
    fn test_setup_request() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.setup_request(6001, 6002);

        assert_eq!(request.method, Method::Setup);
        let transport = request.headers.get("Transport").unwrap();
        assert!(transport.contains("control_port=6001"));
        assert!(transport.contains("timing_port=6002"));
    }

    #[test]
    fn test_transport_parsing() {
        let transport_str = "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
        let transport = RaopRtspSession::parse_transport(transport_str).unwrap();

        assert_eq!(transport.server_port, 6000);
        assert_eq!(transport.control_port, 6001);
        assert_eq!(transport.timing_port, 6002);
    }

    #[test]
    fn test_volume_request() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.set_volume_request(-15.0);

        assert_eq!(request.method, Method::SetParameter);
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("volume:"));
        assert!(body.contains("-15"));
    }
}
```

---

## Integration Tests

### Test: Full RAOP RTSP session flow

```rust
// tests/raop_rtsp_integration.rs

use airplay2_rs::protocol::raop::{RaopRtspSession, RaopSessionState};
use airplay2_rs::protocol::rtsp::{Method, RtspResponse, StatusCode, Headers};

#[test]
fn test_full_session_flow() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);

    // 1. OPTIONS
    let options = session.options_request();
    assert_eq!(options.method, Method::Options);

    // Simulate successful response
    let mut headers = Headers::new();
    headers.insert("CSeq", "1");
    headers.insert("Public", "ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };

    session.process_response(Method::Options, &response).unwrap();
    assert_eq!(session.state(), RaopSessionState::OptionsExchange);

    // 2. ANNOUNCE
    let sdp = session.prepare_announce().unwrap();
    let announce = session.announce_request(&sdp);
    assert_eq!(announce.method, Method::Announce);
    assert!(!announce.body.is_empty());

    // ... continue flow
}
```

---

## Acceptance Criteria

- [ ] SDP parsing handles all RAOP fields correctly
- [ ] SDP generation produces valid output
- [ ] RAOP session state machine transitions correctly
- [ ] All RAOP RTSP methods are implemented
- [ ] Apple-Challenge header is included in OPTIONS
- [ ] Transport header parsing extracts all ports
- [ ] Volume control uses correct dB format
- [ ] Session keys are generated and encoded correctly
- [ ] All unit tests pass
- [ ] Integration tests pass

---

## Notes

- SDP parser should be lenient with whitespace variations
- Some devices may have non-standard SDP attributes
- Transport response format may vary between implementations
- Consider adding support for AAC codec SDP format
- Debug logging should show full request/response for protocol debugging
