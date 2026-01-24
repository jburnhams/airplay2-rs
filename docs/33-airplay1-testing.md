# Section 33: AirPlay 1 Testing Strategy

## Dependencies
- All previous AirPlay 1 sections (24-32)
- **Section 20**: Mock AirPlay Server (for reference)
- **Section 01**: Project Setup & CI/CD (for test infrastructure)

## Overview

Testing AirPlay 1 (RAOP) implementation requires a comprehensive strategy covering unit tests, integration tests, protocol compliance tests, and real device testing. This section extends the existing test infrastructure to support RAOP protocol verification.

## Testing Pyramid

```
                    ┌───────────────┐
                    │    Manual     │
                    │  Device Tests │
                    └───────┬───────┘
                            │
                ┌───────────┴───────────┐
                │   Integration Tests    │
                │   (Mock RAOP Server)   │
                └───────────┬───────────┘
                            │
        ┌───────────────────┴───────────────────┐
        │            Protocol Tests              │
        │  (RTSP, RTP, DMAP, Encryption flows)  │
        └───────────────────┬───────────────────┘
                            │
┌───────────────────────────┴───────────────────────────┐
│                      Unit Tests                        │
│  (Codecs, parsers, encoders, crypto, state machines)  │
└───────────────────────────────────────────────────────┘
```

## Objectives

- Extend mock server to support RAOP protocol
- Create comprehensive unit test suites for all RAOP components
- Implement protocol compliance tests
- Define real device testing procedures
- Establish CI/CD integration for RAOP tests

---

## Tasks

### 33.1 Mock RAOP Server

- [ ] **33.1.1** Implement mock RAOP server

**File:** `src/testing/mock_raop_server.rs`

```rust
//! Mock RAOP server for testing

use crate::protocol::rtp::timing::NtpTimestamp;
use crate::protocol::crypto::rsa::RaopRsaPrivateKey;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// Mock RAOP server state
#[derive(Debug, Clone, Default)]
pub struct MockRaopState {
    /// RTSP session ID
    pub session_id: Option<String>,
    /// Received audio packets
    pub audio_packets: Vec<Vec<u8>>,
    /// Current volume (dB)
    pub volume_db: f32,
    /// Received metadata
    pub metadata: Option<Vec<u8>>,
    /// Received artwork
    pub artwork: Option<Vec<u8>>,
    /// Current playback state
    pub playing: bool,
    /// AES key (decrypted from rsaaeskey)
    pub aes_key: Option<[u8; 16]>,
    /// AES IV
    pub aes_iv: Option<[u8; 16]>,
}

/// Mock RAOP server configuration
#[derive(Debug, Clone)]
pub struct MockRaopConfig {
    /// RTSP port
    pub rtsp_port: u16,
    /// Audio server port
    pub audio_port: u16,
    /// Control port
    pub control_port: u16,
    /// Timing port
    pub timing_port: u16,
    /// Device name
    pub name: String,
    /// MAC address
    pub mac_address: [u8; 6],
    /// Supported codecs
    pub codecs: Vec<u8>,
    /// Supported encryption types
    pub encryption_types: Vec<u8>,
    /// Require Apple-Challenge
    pub require_challenge: bool,
}

impl Default for MockRaopConfig {
    fn default() -> Self {
        Self {
            rtsp_port: 5000,
            audio_port: 6000,
            control_port: 6001,
            timing_port: 6002,
            name: "Mock RAOP".to_string(),
            mac_address: [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            codecs: vec![0, 1, 2], // PCM, ALAC, AAC
            encryption_types: vec![0, 1], // None, RSA
            require_challenge: true,
        }
    }
}

/// Mock RAOP server
pub struct MockRaopServer {
    /// Configuration
    config: MockRaopConfig,
    /// Server state
    state: Arc<Mutex<MockRaopState>>,
    /// RSA private key for authentication
    rsa_key: RaopRsaPrivateKey,
    /// Running state
    running: bool,
}

impl MockRaopServer {
    /// Create new mock server
    pub fn new(config: MockRaopConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(MockRaopState::default())),
            rsa_key: RaopRsaPrivateKey::generate().expect("failed to generate RSA key"),
            running: false,
        }
    }

    /// Get server address for connection
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.config.rtsp_port)
    }

    /// Get mDNS service name
    pub fn service_name(&self) -> String {
        let mac = self.config.mac_address
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        format!("{}@{}", mac, self.config.name)
    }

    /// Get TXT records for mDNS
    pub fn txt_records(&self) -> HashMap<String, String> {
        let mut records = HashMap::new();
        records.insert("txtvers".to_string(), "1".to_string());
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(),
            self.config.codecs.iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        records.insert("et".to_string(),
            self.config.encryption_types.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());
        records
    }

    /// Start the server
    pub async fn start(&mut self) -> Result<(), MockServerError> {
        self.running = true;

        // Start RTSP listener
        // Start UDP listeners for audio, control, timing

        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) {
        self.running = false;
    }

    /// Get current state
    pub fn state(&self) -> MockRaopState {
        self.state.lock().unwrap().clone()
    }

    /// Reset state
    pub fn reset(&self) {
        *self.state.lock().unwrap() = MockRaopState::default();
    }

    /// Get RSA public key (for testing client)
    pub fn public_key(&self) -> rsa::RsaPublicKey {
        self.rsa_key.public_key()
    }

    /// Handle RTSP OPTIONS request
    fn handle_options(
        &self,
        request: &crate::protocol::rtsp::RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{RtspResponse, StatusCode, Headers};

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Public", "ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER");

        // Handle Apple-Challenge if present
        if self.config.require_challenge {
            if let Some(challenge) = request.headers.get("Apple-Challenge") {
                // Generate Apple-Response
                // let response = generate_response(...);
                // headers.insert("Apple-Response", response);
            }
        }

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP ANNOUNCE request
    fn handle_announce(
        &self,
        request: &crate::protocol::rtsp::RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{RtspResponse, StatusCode, Headers};
        use crate::protocol::sdp::SdpParser;

        // Parse SDP
        let sdp_text = String::from_utf8_lossy(&request.body);
        if let Ok(sdp) = SdpParser::parse(&sdp_text) {
            // Extract and decrypt AES key
            if let (Some(rsaaeskey), Some(aesiv)) = (sdp.rsaaeskey(), sdp.aesiv()) {
                if let Ok((_, key, iv)) = crate::protocol::raop::encryption::parse_wrapped_keys(
                    rsaaeskey,
                    aesiv,
                    &self.rsa_key,
                ) {
                    let mut state = self.state.lock().unwrap();
                    state.aes_key = Some(key);
                    state.aes_iv = Some(iv);
                }
            }
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP SETUP request
    fn handle_setup(
        &self,
        request: &crate::protocol::rtsp::RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{RtspResponse, StatusCode, Headers};

        let session_id = format!("{:016X}", rand::random::<u64>());

        {
            let mut state = self.state.lock().unwrap();
            state.session_id = Some(session_id.clone());
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Session", &session_id);
        headers.insert("Transport", format!(
            "RTP/AVP/UDP;unicast;mode=record;server_port={};control_port={};timing_port={}",
            self.config.audio_port,
            self.config.control_port,
            self.config.timing_port,
        ));
        headers.insert("Audio-Latency", "11025");

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP RECORD request
    fn handle_record(
        &self,
        request: &crate::protocol::rtsp::RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{RtspResponse, StatusCode, Headers};

        {
            let mut state = self.state.lock().unwrap();
            state.playing = true;
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Audio-Latency", "11025");

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP SET_PARAMETER request
    fn handle_set_parameter(
        &self,
        request: &crate::protocol::rtsp::RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{RtspResponse, StatusCode, Headers};

        let content_type = request.headers.content_type().unwrap_or("");

        {
            let mut state = self.state.lock().unwrap();

            match content_type {
                "text/parameters" => {
                    // Parse volume
                    let body = String::from_utf8_lossy(&request.body);
                    if let Some(line) = body.lines().find(|l| l.starts_with("volume:")) {
                        if let Some(vol_str) = line.strip_prefix("volume:") {
                            if let Ok(vol) = vol_str.trim().parse::<f32>() {
                                state.volume_db = vol;
                            }
                        }
                    }
                }
                "application/x-dmap-tagged" => {
                    state.metadata = Some(request.body.clone());
                }
                "image/jpeg" | "image/png" => {
                    state.artwork = Some(request.body.clone());
                }
                _ => {}
            }
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }
}

/// Mock server errors
#[derive(Debug, thiserror::Error)]
pub enum MockServerError {
    #[error("bind failed: {0}")]
    BindFailed(String),
    #[error("server not running")]
    NotRunning,
}
```

---

### 33.2 Unit Test Suites

- [ ] **33.2.1** Create test modules for each component

**File:** `tests/raop_unit_tests.rs`

```rust
//! RAOP unit test collection

mod discovery_tests {
    use airplay2_rs::discovery::raop::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_raop_capabilities() {
        let mut records = HashMap::new();
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2".to_string());
        records.insert("et".to_string(), "0,1".to_string());
        records.insert("sr".to_string(), "44100".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert!(caps.supports_codec(RaopCodec::Alac));
        assert!(caps.supports_rsa());
    }

    #[test]
    fn test_service_name_parsing() {
        let (mac, name) = parse_raop_service_name("AABBCCDDEEFF@Living Room").unwrap();
        assert_eq!(mac, "AABBCCDDEEFF");
        assert_eq!(name, "Living Room");
    }
}

mod crypto_tests {
    use airplay2_rs::protocol::crypto::rsa::*;

    #[test]
    fn test_rsa_key_generation() {
        let key = RaopRsaPrivateKey::generate().unwrap();
        let public = key.public_key();
        assert_eq!(public.size(), sizes::MODULUS_BYTES);
    }

    #[test]
    fn test_oaep_roundtrip() {
        let private = RaopRsaPrivateKey::generate().unwrap();
        let public = private.public_key();

        // Encrypt with public
        use rsa::Oaep;
        use sha1::Sha1;
        use rand::rngs::OsRng;

        let plaintext = b"test AES key 16b";
        let padding = Oaep::new::<Sha1>();
        let encrypted = public.encrypt(&mut OsRng, padding, plaintext).unwrap();

        // Decrypt with private
        let decrypted = private.decrypt_oaep(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

mod sdp_tests {
    use airplay2_rs::protocol::sdp::*;

    #[test]
    fn test_parse_raop_sdp() {
        let sdp_text = r#"v=0
o=iTunes 1234567890 1 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.50
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:AAAA
a=aesiv:BBBB
"#;

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.rsaaeskey(), Some("AAAA"));
        assert_eq!(sdp.aesiv(), Some("BBBB"));
    }

    #[test]
    fn test_build_announce_sdp() {
        let sdp = create_raop_announce_sdp(
            "1234567890",
            "192.168.1.100",
            "192.168.1.50",
            "encrypted_key",
            "init_vector",
        );

        assert!(sdp.contains("v=0"));
        assert!(sdp.contains("rsaaeskey:encrypted_key"));
        assert!(sdp.contains("aesiv:init_vector"));
    }
}

mod encryption_tests {
    use airplay2_rs::protocol::raop::encryption::*;

    #[test]
    fn test_aes_ctr_roundtrip() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let encryptor = RaopEncryptor::new(key, iv);
        let decryptor = RaopDecryptor::new(key, iv);

        let original = vec![0xAA; FRAME_SIZE];
        let encrypted = encryptor.encrypt(&original, 0).unwrap();
        let decrypted = decryptor.decrypt(&encrypted, 0).unwrap();

        assert_eq!(decrypted, original);
    }
}

mod rtp_tests {
    use airplay2_rs::protocol::rtp::raop::*;

    #[test]
    fn test_audio_packet_roundtrip() {
        let payload = vec![1, 2, 3, 4, 5];
        let packet = RaopAudioPacket::new(100, 44100, 0x12345678, payload.clone())
            .with_marker();

        let encoded = packet.encode();
        let decoded = RaopAudioPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.timestamp, 44100);
        assert!(decoded.marker);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_sync_packet() {
        use airplay2_rs::protocol::rtp::timing::NtpTimestamp;

        let ntp = NtpTimestamp::now();
        let packet = SyncPacket::new(1000, ntp, 1352, true);

        let encoded = packet.encode();
        let decoded = SyncPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.rtp_timestamp, 1000);
        assert_eq!(decoded.next_timestamp, 1352);
        assert!(decoded.extension);
    }
}

mod dmap_tests {
    use airplay2_rs::protocol::daap::dmap::*;

    #[test]
    fn test_dmap_encoding() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "Test Track");
        encoder.string(DmapTag::SongArtist, "Test Artist");

        let data = encoder.finish();
        let decoded = decode_dmap(&data).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].1, "Test Track");
        assert_eq!(decoded[1].1, "Test Artist");
    }
}

mod progress_tests {
    use airplay2_rs::protocol::daap::progress::*;

    #[test]
    fn test_progress_encode_parse() {
        let progress = PlaybackProgress::new(1000, 2000, 10000);
        let encoded = progress.encode();
        let parsed = PlaybackProgress::parse(&encoded).unwrap();

        assert_eq!(parsed.start, 1000);
        assert_eq!(parsed.current, 2000);
        assert_eq!(parsed.end, 10000);
    }

    #[test]
    fn test_progress_percentage() {
        let progress = PlaybackProgress::new(0, 500, 1000);
        assert!((progress.percentage() - 0.5).abs() < 0.001);
    }
}

mod session_tests {
    use airplay2_rs::protocol::raop::session::*;

    #[test]
    fn test_session_state_machine() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        assert_eq!(session.state(), RaopSessionState::Init);

        // Create OPTIONS request
        let request = session.options_request();
        assert!(request.headers.get("Apple-Challenge").is_some());
    }

    #[test]
    fn test_transport_parsing() {
        let transport = "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
        let parsed = RaopRtspSession::parse_transport(transport).unwrap();

        assert_eq!(parsed.server_port, 6000);
        assert_eq!(parsed.control_port, 6001);
        assert_eq!(parsed.timing_port, 6002);
    }
}
```

---

### 33.3 Integration Tests

- [ ] **33.3.1** Create integration test suite

**File:** `tests/raop_integration_tests.rs`

```rust
//! RAOP integration tests using mock server

use airplay2_rs::testing::mock_raop_server::{MockRaopServer, MockRaopConfig};
use airplay2_rs::client::{UnifiedAirPlayClient, ClientConfig, PreferredProtocol};
use std::time::Duration;

async fn setup_mock_server() -> MockRaopServer {
    let config = MockRaopConfig {
        rtsp_port: 15000 + rand::random::<u16>() % 1000,
        audio_port: 16000 + rand::random::<u16>() % 1000,
        ..Default::default()
    };
    let mut server = MockRaopServer::new(config);
    server.start().await.expect("failed to start mock server");
    server
}

#[tokio::test]
async fn test_full_raop_session() {
    let server = setup_mock_server().await;

    // Create client configured for RAOP
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        connection_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    // Create mock device
    let device = airplay2_rs::discovery::DiscoveredDevice {
        id: "test-device".to_string(),
        name: server.service_name(),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        airplay_port: None,
        raop_port: Some(server.config.rtsp_port),
        airplay_capabilities: None,
        raop_capabilities: Some(Default::default()),
        protocol: airplay2_rs::discovery::DeviceProtocol::RaopOnly,
    };

    // Connect
    client.connect(device).await.expect("connection failed");
    assert!(client.is_connected());

    // Set volume
    client.set_volume(0.5).await.expect("set volume failed");

    // Start playback
    client.play().await.expect("play failed");

    // Verify server state
    let state = server.state();
    assert!(state.playing);
    assert!(state.volume_db < 0.0); // -15dB for 0.5 linear

    // Disconnect
    client.disconnect().await.expect("disconnect failed");
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_raop_audio_streaming() {
    let server = setup_mock_server().await;

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    // ... connect ...

    // Stream some audio
    let audio_frame = vec![0u8; 352 * 4]; // One frame
    for _ in 0..10 {
        client.stream_audio(&audio_frame).await.expect("stream failed");
    }

    // Verify packets received
    let state = server.state();
    assert!(!state.audio_packets.is_empty());

    // Cleanup
    client.disconnect().await.ok();
}

#[tokio::test]
async fn test_raop_metadata() {
    let server = setup_mock_server().await;

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        enable_metadata: true,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    // ... connect ...

    // Set metadata
    let track = airplay2_rs::types::TrackInfo {
        title: Some("Test Song".to_string()),
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        ..Default::default()
    };

    client.session_mut().unwrap()
        .set_metadata(&track)
        .await
        .expect("metadata failed");

    // Verify
    let state = server.state();
    assert!(state.metadata.is_some());

    // Cleanup
    client.disconnect().await.ok();
}

#[tokio::test]
async fn test_raop_encryption() {
    let server = setup_mock_server().await;

    // ... full flow with encryption verification ...

    // Verify AES key was exchanged
    let state = server.state();
    assert!(state.aes_key.is_some());
    assert!(state.aes_iv.is_some());
}
```

---

### 33.4 Protocol Compliance Tests

- [ ] **33.4.1** Create protocol compliance test suite

**File:** `tests/raop_protocol_compliance.rs`

```rust
//! RAOP protocol compliance tests
//!
//! These tests verify that the implementation conforms to the
//! RAOP protocol specification.

use airplay2_rs::protocol::rtsp::*;
use airplay2_rs::protocol::raop::session::*;
use airplay2_rs::protocol::sdp::*;

/// Test RTSP request format compliance
mod rtsp_compliance {
    use super::*;

    #[test]
    fn test_options_request_format() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.options_request();
        let encoded = String::from_utf8_lossy(&request.encode());

        // Must have RTSP/1.0 version
        assert!(encoded.contains("RTSP/1.0"));

        // Must have required headers
        assert!(encoded.contains("CSeq:"));
        assert!(encoded.contains("User-Agent:"));

        // Should have Apple-Challenge for authentication
        assert!(encoded.contains("Apple-Challenge:"));
    }

    #[test]
    fn test_announce_sdp_format() {
        let sdp = create_raop_announce_sdp(
            "1234567890",
            "192.168.1.100",
            "192.168.1.50",
            "encrypted_key",
            "init_vector",
        );

        // Must start with v=0
        assert!(sdp.starts_with("v=0"));

        // Must have origin line
        assert!(sdp.contains("o=iTunes"));

        // Must have connection line
        assert!(sdp.contains("c=IN IP"));

        // Must have media line
        assert!(sdp.contains("m=audio"));

        // Must have encryption attributes
        assert!(sdp.contains("a=rsaaeskey:"));
        assert!(sdp.contains("a=aesiv:"));
    }

    #[test]
    fn test_setup_transport_format() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.setup_request(6001, 6002);
        let encoded = String::from_utf8_lossy(&request.encode());

        // Must have Transport header
        assert!(encoded.contains("Transport:"));

        // Must specify RTP/AVP/UDP
        assert!(encoded.contains("RTP/AVP/UDP"));

        // Must specify mode=record
        assert!(encoded.contains("mode=record"));

        // Must specify ports
        assert!(encoded.contains("control_port=6001"));
        assert!(encoded.contains("timing_port=6002"));
    }

    #[test]
    fn test_cseq_increments() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);

        let r1 = session.options_request();
        let r2 = session.options_request();
        let r3 = session.setup_request(6001, 6002);

        assert_eq!(r1.headers.cseq(), Some(1));
        assert_eq!(r2.headers.cseq(), Some(2));
        assert_eq!(r3.headers.cseq(), Some(3));
    }
}

/// Test RTP packet format compliance
mod rtp_compliance {
    use airplay2_rs::protocol::rtp::raop::*;

    #[test]
    fn test_audio_packet_header() {
        let packet = RaopAudioPacket::new(100, 44100, 0x12345678, vec![0; 100]);
        let encoded = packet.encode();

        // Version must be 2 (bits 6-7 of byte 0)
        assert_eq!((encoded[0] >> 6) & 0x03, 2);

        // Payload type must be 0x60 (realtime) or 0x61 (buffered)
        assert_eq!(encoded[1] & 0x7F, 0x60);

        // Sequence number (bytes 2-3, big-endian)
        assert_eq!(u16::from_be_bytes([encoded[2], encoded[3]]), 100);

        // Timestamp (bytes 4-7, big-endian)
        assert_eq!(u32::from_be_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]), 44100);
    }

    #[test]
    fn test_marker_bit_on_first_packet() {
        let packet = RaopAudioPacket::new(0, 0, 0, vec![])
            .with_marker();
        let encoded = packet.encode();

        // Marker bit is bit 7 of byte 1
        assert_eq!(encoded[1] & 0x80, 0x80);
    }

    #[test]
    fn test_sync_packet_format() {
        use airplay2_rs::protocol::rtp::timing::NtpTimestamp;

        let ntp = NtpTimestamp { seconds: 0x12345678, fraction: 0x9ABCDEF0 };
        let packet = SyncPacket::new(1000, ntp, 1352, true);
        let encoded = packet.encode();

        // Payload type must be 0x54
        assert_eq!(encoded[1] & 0x7F, 0x54);

        // Extension bit set for first sync
        assert_eq!(encoded[0] & 0x10, 0x10);

        // Total size must be 20 bytes
        assert_eq!(encoded.len(), 20);
    }
}

/// Test timing protocol compliance
mod timing_compliance {
    use airplay2_rs::protocol::rtp::raop_timing::*;

    #[test]
    fn test_timing_request_format() {
        let request = RaopTimingRequest::new();
        let encoded = request.encode(1);

        // Must be 32 bytes
        assert_eq!(encoded.len(), 32);

        // Payload type must be 0x52
        assert_eq!(encoded[1] & 0x7F, 0x52);

        // Marker bit should be set
        assert_eq!(encoded[1] & 0x80, 0x80);
    }
}

/// Test DMAP encoding compliance
mod dmap_compliance {
    use airplay2_rs::protocol::daap::dmap::*;

    #[test]
    fn test_dmap_tag_codes() {
        // Verify tag codes match DAAP specification
        assert_eq!(DmapTag::ItemName.code(), b"minm");
        assert_eq!(DmapTag::SongArtist.code(), b"asar");
        assert_eq!(DmapTag::SongAlbum.code(), b"asal");
    }

    #[test]
    fn test_dmap_length_encoding() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "Test");
        let data = encoder.finish();

        // Tag (4) + Length (4) + "Test" (4) = 12 bytes
        assert_eq!(data.len(), 12);

        // Length field (bytes 4-7) should be 4
        assert_eq!(u32::from_be_bytes([data[4], data[5], data[6], data[7]]), 4);
    }
}
```

---

### 33.5 Real Device Testing

- [ ] **33.5.1** Document manual testing procedures

**File:** `tests/README_DEVICE_TESTING.md`

```markdown
# Real Device Testing Procedures

## Supported Test Devices

- AirPort Express (1st/2nd generation)
- Apple TV (2nd/3rd generation)
- HomePod (AirPlay 1 mode)
- Third-party RAOP receivers (Shairport-sync)

## Test Environment Setup

1. **Network Configuration**
   - Test device and computer on same network
   - Multicast traffic enabled (for mDNS)
   - UDP ports 5000-7000 accessible

2. **Test Device Preparation**
   - Reset device to factory defaults
   - No password protection (for initial tests)
   - Connected to audio output (speakers/headphones)

## Manual Test Checklist

### Discovery Tests

- [ ] Device appears in mDNS scan within 5 seconds
- [ ] TXT records parsed correctly
- [ ] MAC address extracted from service name
- [ ] Capabilities match device specifications

### Connection Tests

- [ ] OPTIONS request succeeds
- [ ] Apple-Challenge verified (if required)
- [ ] ANNOUNCE with SDP accepted
- [ ] SETUP returns valid transport parameters
- [ ] RECORD starts without error

### Audio Streaming Tests

- [ ] Audio plays within 500ms of first packet
- [ ] No audible glitches with continuous stream
- [ ] Volume changes take effect
- [ ] Playback stops on TEARDOWN

### Metadata Tests

- [ ] Track title displays on device (if supported)
- [ ] Artist/album displays correctly
- [ ] Artwork displays (if device has screen)
- [ ] Progress bar updates

### Error Recovery Tests

- [ ] Reconnects after network interruption
- [ ] Handles device sleep/wake
- [ ] Recovers from packet loss (retransmission)

## Automated Device Tests

Run with actual device:

```bash
RAOP_TEST_DEVICE=192.168.1.50:5000 cargo test --test raop_device_tests
```

## Known Device Quirks

| Device | Issue | Workaround |
|--------|-------|------------|
| AirPort Express Gen1 | Slow OPTIONS response | Increase timeout to 5s |
| Some Shairport builds | No Apple-Challenge | Disable challenge verification |
| HomePod | Prefers AirPlay 2 | Force RAOP with config |
```

---

## CI/CD Integration

### GitHub Actions Workflow

**File:** `.github/workflows/raop-tests.yml`

```yaml
name: RAOP Tests

on:
  push:
    paths:
      - 'src/protocol/raop/**'
      - 'src/protocol/daap/**'
      - 'src/protocol/sdp/**'
      - 'tests/raop_*'
  pull_request:
    paths:
      - 'src/protocol/raop/**'

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run RAOP unit tests
        run: cargo test --lib raop

  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run RAOP integration tests
        run: cargo test --test raop_integration_tests

  protocol-compliance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run protocol compliance tests
        run: cargo test --test raop_protocol_compliance
```

---

## Acceptance Criteria

- [ ] Mock RAOP server handles all RTSP methods
- [ ] Unit tests cover all RAOP components
- [ ] Integration tests verify full session flow
- [ ] Protocol compliance tests pass
- [ ] CI/CD runs RAOP tests automatically
- [ ] Device testing procedures documented
- [ ] All tests pass on supported platforms

---

## Notes

- Mock server uses random ports to avoid conflicts
- Integration tests may be slower due to network I/O
- Real device tests require manual setup
- Consider adding fuzzing tests for parser robustness
- Property-based tests useful for codec verification
