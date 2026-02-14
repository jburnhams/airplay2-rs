# Section 51: /info Endpoint & Capabilities

## Dependencies
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 47**: AirPlay 2 Service Advertisement (feature flags)
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 03**: Binary Plist Codec

## Overview

The `/info` endpoint is the first request senders make after connecting to an AirPlay 2 receiver. It returns a binary plist containing device capabilities, supported features, and configuration information.

This endpoint is critical for protocol negotiation - senders use this information to determine what features to use and how to configure the streaming session.

### Request/Response

```
GET /info HTTP/1.1
-> 200 OK
   Content-Type: application/x-apple-binary-plist
   Body: { device capabilities plist }
```

## Objectives

- Implement GET /info handler
- Generate comprehensive device capabilities plist
- Support all required and optional capability fields
- Allow dynamic capability updates
- Ensure compatibility with iOS/macOS senders

---

## Tasks

### 51.1 Capability Types

- [ ] **51.1.1** Define capability structures

**File:** `src/receiver/ap2/capabilities.rs`

```rust
//! Device capabilities for AirPlay 2 receiver
//!
//! These structures define what our receiver advertises to senders
//! via the /info endpoint.

use crate::protocol::plist::PlistValue;
use std::collections::HashMap;

/// Device capabilities for /info response
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    /// Device identification
    pub device_id: String,
    pub name: String,
    pub model: String,
    pub manufacturer: String,
    pub serial_number: Option<String>,

    /// Version information
    pub source_version: String,
    pub protocol_version: String,
    pub firmware_version: String,
    pub os_build_version: Option<String>,

    /// Feature flags (64-bit)
    pub features: u64,

    /// Status flags (32-bit)
    pub status_flags: u32,

    /// Public key for pairing (Ed25519)
    pub public_key: [u8; 32],

    /// Pairing identity (UUID)
    pub pairing_identity: String,

    /// Audio capabilities
    pub audio_formats: Vec<AudioFormatCapability>,
    pub audio_latencies: AudioLatencies,

    /// Display capabilities (for screen mirroring, optional)
    pub displays: Vec<DisplayCapability>,

    /// Volume control
    pub initial_volume: f32,
    pub supports_volume: bool,

    /// Group/multi-room
    pub group_uuid: Option<String>,
    pub group_leader: bool,

    /// Timing
    pub supports_ptp: bool,
    pub ptp_clock_id: Option<u64>,

    /// Authentication requirements
    pub requires_password: bool,
    pub supports_homekit: bool,
}

/// Audio format capability
#[derive(Debug, Clone)]
pub struct AudioFormatCapability {
    /// Format type (96 = ALAC, 97 = AAC-ELD, etc.)
    pub type_id: u32,
    /// Audio channels
    pub channels: u8,
    /// Sample rates (Hz)
    pub sample_rates: Vec<u32>,
    /// Bits per sample
    pub bits_per_sample: Vec<u8>,
    /// Encryption types supported
    pub encryption_types: Vec<EncryptionType>,
}

/// Encryption types for audio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionType {
    /// No encryption
    None = 0,
    /// RSA (AirPlay 1)
    Rsa = 1,
    /// FairPlay
    FairPlay = 2,
    /// MFi SAP
    MfiSap = 3,
    /// FairPlay SAP
    FairPlaySap = 4,
}

/// Audio latency configuration
#[derive(Debug, Clone)]
pub struct AudioLatencies {
    /// Minimum supported latency (ms)
    pub min_latency_ms: u32,
    /// Maximum supported latency (ms)
    pub max_latency_ms: u32,
    /// Default latency (ms)
    pub default_latency_ms: u32,
    /// Latency for buffered (multi-room) mode
    pub buffered_latency_ms: u32,
}

/// Display capability (for screen mirroring)
#[derive(Debug, Clone)]
pub struct DisplayCapability {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f32,
    pub uuid: String,
    pub features: u32,
}

impl Default for AudioLatencies {
    fn default() -> Self {
        Self {
            min_latency_ms: 0,
            max_latency_ms: 4000,
            default_latency_ms: 2000,
            buffered_latency_ms: 2000,
        }
    }
}

impl DeviceCapabilities {
    /// Create default capabilities for an audio-only receiver
    pub fn audio_receiver(
        device_id: &str,
        name: &str,
        public_key: [u8; 32],
    ) -> Self {
        Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            model: "Receiver1,1".to_string(),
            manufacturer: "airplay2-rs".to_string(),
            serial_number: None,

            source_version: "366.0".to_string(),
            protocol_version: "1.1".to_string(),
            firmware_version: env!("CARGO_PKG_VERSION").to_string(),
            os_build_version: None,

            features: Self::default_audio_features(),
            status_flags: 0x04,  // Supports PIN

            public_key,
            pairing_identity: Self::derive_pairing_identity(device_id),

            audio_formats: Self::default_audio_formats(),
            audio_latencies: AudioLatencies::default(),

            displays: vec![],

            initial_volume: 0.5,
            supports_volume: true,

            group_uuid: None,
            group_leader: false,

            supports_ptp: true,
            ptp_clock_id: None,

            requires_password: false,
            supports_homekit: true,
        }
    }

    /// Default feature flags for audio receiver
    fn default_audio_features() -> u64 {
        let mut features: u64 = 0;

        // Core audio
        features |= 1 << 9;   // Audio
        features |= 1 << 11;  // Audio redundant

        // Audio formats
        features |= 1 << 22;  // ALAC
        features |= 1 << 23;  // AAC-LC
        features |= 1 << 25;  // AAC-ELD

        // Authentication
        features |= 1 << 14;  // Auth setup
        features |= 1 << 17;  // Legacy pairing
        features |= 1 << 26;  // PIN
        features |= 1 << 27;  // Transient pairing
        features |= 1 << 46;  // HomeKit

        // Timing
        features |= 1 << 38;  // Buffered audio
        features |= 1 << 40;  // PTP

        // Control
        features |= 1 << 18;  // Unified media control
        features |= 1 << 19;  // Volume control

        features
    }

    /// Default audio formats for receiver
    fn default_audio_formats() -> Vec<AudioFormatCapability> {
        vec![
            // ALAC
            AudioFormatCapability {
                type_id: 96,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16, 24],
                encryption_types: vec![EncryptionType::None],
            },
            // AAC-LC
            AudioFormatCapability {
                type_id: 97,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16],
                encryption_types: vec![EncryptionType::None],
            },
            // AAC-ELD
            AudioFormatCapability {
                type_id: 98,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16],
                encryption_types: vec![EncryptionType::None],
            },
            // PCM
            AudioFormatCapability {
                type_id: 100,
                channels: 2,
                sample_rates: vec![44100, 48000, 96000],
                bits_per_sample: vec![16, 24],
                encryption_types: vec![EncryptionType::None],
            },
        ]
    }

    /// Derive pairing identity from device ID
    fn derive_pairing_identity(device_id: &str) -> String {
        use sha2::{Sha256, Digest};

        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update(b"AirPlay2-PI");
        let hash = hasher.finalize();

        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            hash[0], hash[1], hash[2], hash[3],
            hash[4], hash[5],
            (hash[6] & 0x0f) | 0x40, hash[7],
            (hash[8] & 0x3f) | 0x80, hash[9],
            hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
        )
    }

    /// Convert to binary plist value
    pub fn to_plist(&self) -> PlistValue {
        let mut dict: HashMap<String, PlistValue> = HashMap::new();

        // Device identification
        dict.insert("deviceid".to_string(), PlistValue::String(self.device_id.clone()));
        dict.insert("name".to_string(), PlistValue::String(self.name.clone()));
        dict.insert("model".to_string(), PlistValue::String(self.model.clone()));
        dict.insert("manufacturer".to_string(), PlistValue::String(self.manufacturer.clone()));

        if let Some(ref serial) = self.serial_number {
            dict.insert("serialNumber".to_string(), PlistValue::String(serial.clone()));
        }

        // Version information
        dict.insert("srcvers".to_string(), PlistValue::String(self.source_version.clone()));
        dict.insert("protovers".to_string(), PlistValue::String(self.protocol_version.clone()));
        dict.insert("fv".to_string(), PlistValue::String(self.firmware_version.clone()));

        if let Some(ref os_build) = self.os_build_version {
            dict.insert("osBuildVersion".to_string(), PlistValue::String(os_build.clone()));
        }

        // Feature flags
        dict.insert("features".to_string(), PlistValue::Integer(self.features as i64));
        dict.insert("statusFlags".to_string(), PlistValue::Integer(self.status_flags as i64));

        // Public key
        dict.insert("pk".to_string(), PlistValue::Data(self.public_key.to_vec()));

        // Pairing identity
        dict.insert("pi".to_string(), PlistValue::String(self.pairing_identity.clone()));

        // Audio formats
        dict.insert("audioFormats".to_string(), self.audio_formats_to_plist());

        // Audio latencies
        dict.insert("audioLatencies".to_string(), self.audio_latencies_to_plist());

        // Volume
        dict.insert("initialVolume".to_string(), PlistValue::Real(self.initial_volume as f64));

        // Timing
        if self.supports_ptp {
            dict.insert("supportsPTP".to_string(), PlistValue::Boolean(true));
            if let Some(clock_id) = self.ptp_clock_id {
                dict.insert("ptpClockID".to_string(), PlistValue::Integer(clock_id as i64));
            }
        }

        // Group
        if let Some(ref group_uuid) = self.group_uuid {
            dict.insert("groupUUID".to_string(), PlistValue::String(group_uuid.clone()));
        }
        dict.insert("isGroupLeader".to_string(), PlistValue::Boolean(self.group_leader));

        PlistValue::Dict(dict)
    }

    fn audio_formats_to_plist(&self) -> PlistValue {
        let formats: Vec<PlistValue> = self.audio_formats.iter().map(|fmt| {
            let mut dict: HashMap<String, PlistValue> = HashMap::new();
            dict.insert("type".to_string(), PlistValue::Integer(fmt.type_id as i64));
            dict.insert("ch".to_string(), PlistValue::Integer(fmt.channels as i64));

            let sample_rates: Vec<PlistValue> = fmt.sample_rates.iter()
                .map(|&sr| PlistValue::Integer(sr as i64))
                .collect();
            dict.insert("sr".to_string(), PlistValue::Array(sample_rates));

            let bits: Vec<PlistValue> = fmt.bits_per_sample.iter()
                .map(|&b| PlistValue::Integer(b as i64))
                .collect();
            dict.insert("ss".to_string(), PlistValue::Array(bits));

            let enc_types: Vec<PlistValue> = fmt.encryption_types.iter()
                .map(|&e| PlistValue::Integer(e as i64))
                .collect();
            dict.insert("et".to_string(), PlistValue::Array(enc_types));

            PlistValue::Dict(dict)
        }).collect();

        PlistValue::Array(formats)
    }

    fn audio_latencies_to_plist(&self) -> PlistValue {
        let mut latency_entry: HashMap<String, PlistValue> = HashMap::new();
        latency_entry.insert("inputLatencyMicros".to_string(), PlistValue::Integer(0));
        latency_entry.insert("outputLatencyMicros".to_string(),
            PlistValue::Integer((self.audio_latencies.default_latency_ms * 1000) as i64));
        latency_entry.insert("type".to_string(), PlistValue::Integer(96));  // Default format
        latency_entry.insert("audioType".to_string(), PlistValue::String("default".to_string()));

        PlistValue::Array(vec![PlistValue::Dict(latency_entry)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_to_plist() {
        let caps = DeviceCapabilities::audio_receiver(
            "AA:BB:CC:DD:EE:FF",
            "Test Speaker",
            [0u8; 32],
        );

        let plist = caps.to_plist();

        if let PlistValue::Dict(dict) = plist {
            assert!(dict.contains_key("deviceid"));
            assert!(dict.contains_key("name"));
            assert!(dict.contains_key("features"));
            assert!(dict.contains_key("audioFormats"));
            assert!(dict.contains_key("pk"));
        } else {
            panic!("Expected Dict");
        }
    }

    #[test]
    fn test_pairing_identity_deterministic() {
        let pi1 = DeviceCapabilities::derive_pairing_identity("AA:BB:CC:DD:EE:FF");
        let pi2 = DeviceCapabilities::derive_pairing_identity("AA:BB:CC:DD:EE:FF");

        assert_eq!(pi1, pi2);
        assert_eq!(pi1.len(), 36);  // UUID format
    }
}
```

---

### 51.2 /info Handler

- [ ] **51.2.1** Implement the /info endpoint handler

**File:** `src/receiver/ap2/info_endpoint.rs`

```rust
//! GET /info endpoint handler
//!
//! Returns device capabilities to connecting senders.

use crate::protocol::rtsp::RtspRequest;
use super::capabilities::DeviceCapabilities;
use super::request_handler::{Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::body_handler::encode_bplist_body;
use crate::protocol::rtsp::StatusCode;
use std::sync::Arc;

/// Handler for GET /info endpoint
pub struct InfoEndpoint {
    /// Device capabilities
    capabilities: Arc<DeviceCapabilities>,
}

impl InfoEndpoint {
    /// Create a new info endpoint handler
    pub fn new(capabilities: DeviceCapabilities) -> Self {
        Self {
            capabilities: Arc::new(capabilities),
        }
    }

    /// Handle GET /info request
    pub fn handle(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext,
    ) -> Ap2HandleResult {
        log::debug!("Handling GET /info request");

        // Check for qualifier header (optional, indicates specific info requested)
        let qualifier = request.headers.get("X-Apple-Info-Qualifier");

        // Generate capabilities plist
        let plist = if let Some(_qualifier) = qualifier {
            // Could filter based on qualifier
            self.capabilities.to_plist()
        } else {
            self.capabilities.to_plist()
        };

        // Encode to binary plist
        let body = match encode_bplist_body(&plist) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to encode /info response: {}", e);
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Failed to encode response: {}", e)),
                };
            }
        };

        log::debug!("/info response: {} bytes", body.len());

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .header("Content-Type", "application/x-apple-binary-plist")
                .binary_body(body)
                .encode(),
            new_state: Some(super::session_state::Ap2SessionState::InfoExchanged),
            event: None,
            error: None,
        }
    }

    /// Update capabilities (e.g., when configuration changes)
    pub fn update_capabilities(&mut self, capabilities: DeviceCapabilities) {
        self.capabilities = Arc::new(capabilities);
    }

    /// Get current capabilities
    pub fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }
}

/// Create handler function for request router
pub fn create_info_handler(
    endpoint: Arc<InfoEndpoint>,
) -> impl Fn(&RtspRequest, u32, &Ap2RequestContext) -> Ap2HandleResult {
    move |req, cseq, ctx| endpoint.handle(req, cseq, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::rtsp::{Method, Headers};

    fn make_info_request() -> RtspRequest {
        let mut headers = Headers::new();
        headers.insert("CSeq".to_string(), "1".to_string());

        RtspRequest {
            method: Method::Get,
            uri: "/info".to_string(),
            headers,
            body: vec![],
        }
    }

    #[test]
    fn test_info_response() {
        let caps = DeviceCapabilities::audio_receiver(
            "AA:BB:CC:DD:EE:FF",
            "Test Speaker",
            [0u8; 32],
        );
        let endpoint = InfoEndpoint::new(caps);

        let request = make_info_request();
        let context = Ap2RequestContext {
            state: &super::super::session_state::Ap2SessionState::Connected,
            session_id: None,
            encrypted: false,
            decrypt: None,
        };

        let result = endpoint.handle(&request, 1, &context);

        let response_str = String::from_utf8_lossy(&result.response);
        assert!(response_str.contains("200 OK"));
        assert!(response_str.contains("application/x-apple-binary-plist"));
    }

    #[test]
    fn test_state_transition() {
        let caps = DeviceCapabilities::audio_receiver(
            "AA:BB:CC:DD:EE:FF",
            "Test Speaker",
            [0u8; 32],
        );
        let endpoint = InfoEndpoint::new(caps);

        let request = make_info_request();
        let context = Ap2RequestContext {
            state: &super::super::session_state::Ap2SessionState::Connected,
            session_id: None,
            encrypted: false,
            decrypt: None,
        };

        let result = endpoint.handle(&request, 1, &context);

        assert!(matches!(
            result.new_state,
            Some(super::super::session_state::Ap2SessionState::InfoExchanged)
        ));
    }
}
```

---

## Acceptance Criteria

- [ ] /info endpoint returns binary plist response
- [ ] All required capability fields are present
- [ ] Audio formats advertised match configuration
- [ ] Feature flags match advertised capabilities
- [ ] Public key matches advertisement TXT record
- [ ] Pairing identity is consistent
- [ ] Response compatible with iOS/macOS senders
- [ ] All unit tests pass

---

## Notes

### Required Fields

iOS/macOS senders expect these fields at minimum:
- `deviceid` - Unique device identifier
- `features` - 64-bit feature flags
- `model` - Device model string
- `pk` - Ed25519 public key
- `pi` - Pairing identity UUID
- `srcvers` - Source/protocol version

### Optional Fields

These enhance functionality but aren't strictly required:
- `audioFormats` - Detailed format support
- `audioLatencies` - Latency information
- `displays` - Screen mirroring support
- `groupUUID` - Multi-room group

---

## References

- [AirPlay 2 /info Analysis](https://emanuelecozzi.net/docs/airplay2/info/)
- [Section 03: Binary Plist Codec](./complete/03-binary-plist-codec.md)
