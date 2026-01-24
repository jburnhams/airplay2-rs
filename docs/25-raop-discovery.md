# Section 25: RAOP Service Discovery

## Dependencies
- **Section 08**: mDNS Discovery (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 24**: AirPlay 1 Overview (should be reviewed)

## Overview

AirPlay 1 devices (AirPort Express, older Apple TVs, third-party receivers) advertise themselves via the `_raop._tcp` mDNS service type, distinct from AirPlay 2's `_airplay._tcp`. This section extends the existing mDNS discovery infrastructure to detect and parse RAOP service advertisements.

## Objectives

- Extend service browser to discover `_raop._tcp` services
- Parse RAOP-specific TXT records
- Detect device capabilities (codecs, encryption types)
- Distinguish between AirPlay 1-only and dual-protocol devices
- Integrate with existing `AirPlayDevice` type

---

## Tasks

### 25.1 RAOP Service Types

- [ ] **25.1.1** Define RAOP service constants and types

**File:** `src/discovery/raop.rs`

```rust
//! RAOP (AirPlay 1) service discovery

/// RAOP service type for mDNS discovery
pub const RAOP_SERVICE_TYPE: &str = "_raop._tcp.local.";

/// RAOP TXT record keys
pub mod txt_keys {
    /// TXT record version (usually "1")
    pub const TXTVERS: &str = "txtvers";
    /// Number of audio channels (e.g., "2" for stereo)
    pub const CHANNELS: &str = "ch";
    /// Supported codecs (e.g., "0,1,2,3")
    pub const CODECS: &str = "cn";
    /// Metadata support flag
    pub const METADATA: &str = "da";
    /// Supported encryption types (e.g., "0,1,3,5")
    pub const ENCRYPTION: &str = "et";
    /// Supported metadata types (e.g., "0,1,2")
    pub const METADATA_TYPES: &str = "md";
    /// Password required flag
    pub const PASSWORD: &str = "pw";
    /// Sample rate in Hz (e.g., "44100")
    pub const SAMPLE_RATE: &str = "sr";
    /// Sample size in bits (e.g., "16")
    pub const SAMPLE_SIZE: &str = "ss";
    /// Transport protocol (e.g., "UDP")
    pub const TRANSPORT: &str = "tp";
    /// Server version (e.g., "130.14")
    pub const VERSION: &str = "vs";
    /// Version number (e.g., "65537")
    pub const VERSION_NUM: &str = "vn";
    /// Device model (e.g., "AppleTV2,1")
    pub const MODEL: &str = "am";
    /// Status flags
    pub const FLAGS: &str = "sf";
}

/// Supported audio codecs for RAOP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopCodec {
    /// Uncompressed PCM
    Pcm = 0,
    /// Apple Lossless Audio Codec
    Alac = 1,
    /// Advanced Audio Coding
    Aac = 2,
    /// AAC Enhanced Low Delay (for screen mirroring)
    AacEld = 3,
}

impl RaopCodec {
    /// Parse from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Pcm),
            1 => Some(Self::Alac),
            2 => Some(Self::Aac),
            3 => Some(Self::AacEld),
            _ => None,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pcm => "PCM",
            Self::Alac => "Apple Lossless",
            Self::Aac => "AAC",
            Self::AacEld => "AAC-ELD",
        }
    }
}

/// Supported encryption types for RAOP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopEncryption {
    /// No encryption
    None = 0,
    /// RSA (AirPort Express original)
    Rsa = 1,
    /// FairPlay (iTunes DRM)
    FairPlay = 3,
    /// MFi-SAP (third-party devices)
    MfiSap = 4,
    /// FairPlay SAPv2.5 (iOS/macOS mirroring)
    FairPlaySap25 = 5,
}

impl RaopEncryption {
    /// Parse from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Rsa),
            3 => Some(Self::FairPlay),
            4 => Some(Self::MfiSap),
            5 => Some(Self::FairPlaySap25),
            _ => None,
        }
    }

    /// Check if this encryption type is supported by the library
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::None | Self::Rsa)
    }
}

/// Metadata types supported by RAOP devices
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopMetadataType {
    /// Text metadata (track, artist, album)
    Text = 0,
    /// Artwork images
    Artwork = 1,
    /// Playback progress
    Progress = 2,
}

impl RaopMetadataType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Text),
            1 => Some(Self::Artwork),
            2 => Some(Self::Progress),
            _ => None,
        }
    }
}
```

---

### 25.2 RAOP Capabilities Parsing

- [ ] **25.2.1** Implement RAOP TXT record parser

**File:** `src/discovery/raop.rs` (continued)

```rust
/// RAOP device capabilities parsed from TXT records
#[derive(Debug, Clone, Default)]
pub struct RaopCapabilities {
    /// TXT record version
    pub txt_version: u8,
    /// Number of audio channels
    pub channels: u8,
    /// Supported codecs
    pub codecs: Vec<RaopCodec>,
    /// Supported encryption types
    pub encryption_types: Vec<RaopEncryption>,
    /// Supports metadata
    pub metadata_support: bool,
    /// Supported metadata types
    pub metadata_types: Vec<RaopMetadataType>,
    /// Password required
    pub password_required: bool,
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Sample size (bits)
    pub sample_size: u8,
    /// Transport protocol
    pub transport: String,
    /// Server version string
    pub server_version: Option<String>,
    /// Device model
    pub model: Option<String>,
    /// Status flags
    pub status_flags: u32,
}

impl RaopCapabilities {
    /// Parse from TXT record map
    pub fn from_txt_records(records: &std::collections::HashMap<String, String>) -> Self {
        let mut caps = Self::default();

        // Parse txtvers
        if let Some(v) = records.get(txt_keys::TXTVERS) {
            caps.txt_version = v.parse().unwrap_or(1);
        }

        // Parse channels
        if let Some(v) = records.get(txt_keys::CHANNELS) {
            caps.channels = v.parse().unwrap_or(2);
        } else {
            caps.channels = 2; // Default stereo
        }

        // Parse codecs (comma-separated list)
        if let Some(v) = records.get(txt_keys::CODECS) {
            caps.codecs = Self::parse_codec_list(v);
        }

        // Parse encryption types
        if let Some(v) = records.get(txt_keys::ENCRYPTION) {
            caps.encryption_types = Self::parse_encryption_list(v);
        }

        // Parse metadata support
        if let Some(v) = records.get(txt_keys::METADATA) {
            caps.metadata_support = v == "true" || v == "1";
        }

        // Parse metadata types
        if let Some(v) = records.get(txt_keys::METADATA_TYPES) {
            caps.metadata_types = Self::parse_metadata_types(v);
        }

        // Parse password requirement
        if let Some(v) = records.get(txt_keys::PASSWORD) {
            caps.password_required = v == "true" || v == "1";
        }

        // Parse sample rate
        if let Some(v) = records.get(txt_keys::SAMPLE_RATE) {
            caps.sample_rate = v.parse().unwrap_or(44100);
        } else {
            caps.sample_rate = 44100;
        }

        // Parse sample size
        if let Some(v) = records.get(txt_keys::SAMPLE_SIZE) {
            caps.sample_size = v.parse().unwrap_or(16);
        } else {
            caps.sample_size = 16;
        }

        // Parse transport
        if let Some(v) = records.get(txt_keys::TRANSPORT) {
            caps.transport = v.clone();
        } else {
            caps.transport = "UDP".to_string();
        }

        // Optional fields
        caps.server_version = records.get(txt_keys::VERSION).cloned();
        caps.model = records.get(txt_keys::MODEL).cloned();

        if let Some(v) = records.get(txt_keys::FLAGS) {
            caps.status_flags = u32::from_str_radix(v.trim_start_matches("0x"), 16)
                .unwrap_or(0);
        }

        caps
    }

    fn parse_codec_list(s: &str) -> Vec<RaopCodec> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopCodec::from_u8)
            .collect()
    }

    fn parse_encryption_list(s: &str) -> Vec<RaopEncryption> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopEncryption::from_u8)
            .collect()
    }

    fn parse_metadata_types(s: &str) -> Vec<RaopMetadataType> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopMetadataType::from_u8)
            .collect()
    }

    /// Check if device supports a specific codec
    pub fn supports_codec(&self, codec: RaopCodec) -> bool {
        self.codecs.contains(&codec)
    }

    /// Check if device supports RSA encryption
    pub fn supports_rsa(&self) -> bool {
        self.encryption_types.contains(&RaopEncryption::Rsa)
    }

    /// Check if device supports unencrypted streaming
    pub fn supports_unencrypted(&self) -> bool {
        self.encryption_types.contains(&RaopEncryption::None)
    }

    /// Get preferred codec (ALAC > AAC > PCM)
    pub fn preferred_codec(&self) -> Option<RaopCodec> {
        if self.codecs.contains(&RaopCodec::Alac) {
            Some(RaopCodec::Alac)
        } else if self.codecs.contains(&RaopCodec::Aac) {
            Some(RaopCodec::Aac)
        } else if self.codecs.contains(&RaopCodec::Pcm) {
            Some(RaopCodec::Pcm)
        } else {
            self.codecs.first().copied()
        }
    }

    /// Get preferred encryption (RSA if available, else None)
    pub fn preferred_encryption(&self) -> Option<RaopEncryption> {
        if self.supports_rsa() {
            Some(RaopEncryption::Rsa)
        } else if self.supports_unencrypted() {
            Some(RaopEncryption::None)
        } else {
            None
        }
    }
}
```

---

### 25.3 RAOP Service Browser

- [ ] **25.3.1** Extend discovery browser for RAOP services

**File:** `src/discovery/browser.rs` (extensions)

```rust
use super::raop::{RaopCapabilities, RAOP_SERVICE_TYPE};

/// Extended discovery options for both AirPlay 1 and 2
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Discover AirPlay 2 devices (_airplay._tcp)
    pub discover_airplay2: bool,
    /// Discover AirPlay 1/RAOP devices (_raop._tcp)
    pub discover_raop: bool,
    /// Timeout for discovery scan
    pub timeout: Duration,
    /// Filter by device capabilities
    pub filter: Option<DeviceFilter>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            discover_airplay2: true,
            discover_raop: true,
            timeout: Duration::from_secs(5),
            filter: None,
        }
    }
}

/// Device filter criteria
#[derive(Debug, Clone, Default)]
pub struct DeviceFilter {
    /// Require audio support
    pub audio_only: bool,
    /// Require specific codec support
    pub required_codec: Option<RaopCodec>,
    /// Exclude password-protected devices
    pub exclude_password_protected: bool,
}

/// Extended device information with RAOP capabilities
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    /// Device identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// IP addresses
    pub addresses: Vec<std::net::IpAddr>,
    /// AirPlay 2 service port (if available)
    pub airplay_port: Option<u16>,
    /// RAOP service port (if available)
    pub raop_port: Option<u16>,
    /// AirPlay 2 capabilities (if available)
    pub airplay_capabilities: Option<crate::types::DeviceCapabilities>,
    /// RAOP capabilities (if available)
    pub raop_capabilities: Option<RaopCapabilities>,
    /// Detected protocol support
    pub protocol: DeviceProtocol,
}

/// Protocol support detected for a device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceProtocol {
    /// Only AirPlay 2 supported
    AirPlay2Only,
    /// Only AirPlay 1 (RAOP) supported
    RaopOnly,
    /// Both protocols supported
    Both,
}

impl DiscoveredDevice {
    /// Check if device supports AirPlay 2
    pub fn supports_airplay2(&self) -> bool {
        matches!(self.protocol, DeviceProtocol::AirPlay2Only | DeviceProtocol::Both)
    }

    /// Check if device supports RAOP (AirPlay 1)
    pub fn supports_raop(&self) -> bool {
        matches!(self.protocol, DeviceProtocol::RaopOnly | DeviceProtocol::Both)
    }

    /// Get the preferred connection port
    pub fn preferred_port(&self) -> Option<u16> {
        // Prefer AirPlay 2 if available
        self.airplay_port.or(self.raop_port)
    }

    /// Convert to AirPlayDevice for use with client
    pub fn to_airplay_device(&self) -> crate::types::AirPlayDevice {
        crate::types::AirPlayDevice {
            id: self.id.clone(),
            name: self.name.clone(),
            addresses: self.addresses.clone(),
            port: self.preferred_port().unwrap_or(7000),
            capabilities: self.airplay_capabilities.clone()
                .unwrap_or_default(),
            // Extended fields for RAOP support
            raop_port: self.raop_port,
            raop_capabilities: self.raop_capabilities.clone(),
        }
    }
}
```

---

### 25.4 RAOP Service Name Parsing

- [ ] **25.4.1** Parse RAOP service instance names

**File:** `src/discovery/raop.rs` (continued)

```rust
/// Parse RAOP service instance name
///
/// RAOP service names follow the format: `{MAC_ADDRESS}@{DEVICE_NAME}`
/// Example: "0050C212A23F@Living Room"
pub fn parse_raop_service_name(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.splitn(2, '@').collect();
    if parts.len() == 2 {
        let mac = parts[0].to_uppercase();
        let device_name = parts[1].to_string();

        // Validate MAC address format (12 hex characters)
        if mac.len() == 12 && mac.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((mac, device_name));
        }
    }
    None
}

/// Format MAC address with colons
pub fn format_mac_address(mac: &str) -> String {
    mac.chars()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_raop_service_name() {
        let (mac, name) = parse_raop_service_name("0050C212A23F@Living Room").unwrap();
        assert_eq!(mac, "0050C212A23F");
        assert_eq!(name, "Living Room");
    }

    #[test]
    fn test_parse_raop_service_name_with_special_chars() {
        let (mac, name) = parse_raop_service_name("AABBCCDDEEFF@Speaker's Room").unwrap();
        assert_eq!(mac, "AABBCCDDEEFF");
        assert_eq!(name, "Speaker's Room");
    }

    #[test]
    fn test_format_mac_address() {
        assert_eq!(format_mac_address("0050C212A23F"), "00:50:C2:12:A2:3F");
    }
}
```

---

### 25.5 Unified Discovery API

- [ ] **25.5.1** Implement unified discovery stream

**File:** `src/discovery/mod.rs` (extensions)

```rust
/// Discovery event types
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// New device discovered
    DeviceFound(DiscoveredDevice),
    /// Device went offline
    DeviceLost { id: String },
    /// Device information updated
    DeviceUpdated(DiscoveredDevice),
    /// Discovery error (non-fatal)
    Error(String),
}

/// Start continuous discovery for both AirPlay 1 and 2 devices
pub async fn discover_all(options: DiscoveryOptions) -> impl Stream<Item = DiscoveryEvent> {
    // Implementation combines both service browsers
    // and correlates devices that advertise both services
    todo!()
}

/// One-shot scan for all compatible devices
pub async fn scan_all(options: DiscoveryOptions) -> Result<Vec<DiscoveredDevice>, AirPlayError> {
    let events = discover_all(options.clone());
    tokio::pin!(events);

    let mut devices: HashMap<String, DiscoveredDevice> = HashMap::new();
    let deadline = tokio::time::Instant::now() + options.timeout;

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.next()).await {
            Ok(Some(DiscoveryEvent::DeviceFound(device))) => {
                devices.insert(device.id.clone(), device);
            }
            Ok(Some(DiscoveryEvent::DeviceUpdated(device))) => {
                devices.insert(device.id.clone(), device);
            }
            Ok(Some(DiscoveryEvent::DeviceLost { id })) => {
                devices.remove(&id);
            }
            Ok(Some(DiscoveryEvent::Error(_))) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    Ok(devices.into_values().collect())
}
```

---

## Unit Tests

### Test File: `src/discovery/raop.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_capabilities_basic() {
        let mut records = HashMap::new();
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2".to_string());
        records.insert("et".to_string(), "0,1".to_string());
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert_eq!(caps.sample_size, 16);
        assert!(caps.supports_codec(RaopCodec::Pcm));
        assert!(caps.supports_codec(RaopCodec::Alac));
        assert!(caps.supports_codec(RaopCodec::Aac));
        assert!(caps.supports_rsa());
        assert!(caps.supports_unencrypted());
    }

    #[test]
    fn test_parse_capabilities_airport_express() {
        // Typical AirPort Express TXT records
        let mut records = HashMap::new();
        records.insert("txtvers".to_string(), "1".to_string());
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2,3".to_string());
        records.insert("da".to_string(), "true".to_string());
        records.insert("et".to_string(), "0,3,5".to_string());
        records.insert("md".to_string(), "0,1,2".to_string());
        records.insert("pw".to_string(), "false".to_string());
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());
        records.insert("tp".to_string(), "UDP".to_string());
        records.insert("vs".to_string(), "130.14".to_string());
        records.insert("am".to_string(), "AirPort10,115".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert!(caps.metadata_support);
        assert!(!caps.password_required);
        assert_eq!(caps.model, Some("AirPort10,115".to_string()));
        assert_eq!(caps.preferred_codec(), Some(RaopCodec::Alac));
    }

    #[test]
    fn test_preferred_encryption_rsa() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "0,1".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::Rsa));
    }

    #[test]
    fn test_preferred_encryption_none_only() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "0".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::None));
    }

    #[test]
    fn test_preferred_encryption_fairplay_unsupported() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "3".to_string()); // FairPlay only

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), None);
    }

    #[test]
    fn test_codec_preference() {
        let mut records = HashMap::new();
        records.insert("cn".to_string(), "0,2".to_string()); // PCM and AAC

        let caps = RaopCapabilities::from_txt_records(&records);

        // Should prefer AAC over PCM
        assert_eq!(caps.preferred_codec(), Some(RaopCodec::Aac));
    }

    #[test]
    fn test_empty_records() {
        let records = HashMap::new();
        let caps = RaopCapabilities::from_txt_records(&records);

        // Should use sensible defaults
        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert_eq!(caps.sample_size, 16);
        assert_eq!(caps.transport, "UDP");
    }
}
```

---

## Integration Tests

### Test: Discovery of RAOP devices

```rust
// tests/discovery_raop_integration.rs

use airplay2_rs::discovery::{discover_all, DiscoveryOptions, DiscoveryEvent, DeviceProtocol};
use std::time::Duration;
use futures::StreamExt;

#[tokio::test]
async fn test_raop_discovery_simulation() {
    // Use mock mDNS responses
    let options = DiscoveryOptions {
        discover_airplay2: false,
        discover_raop: true,
        timeout: Duration::from_secs(2),
        filter: None,
    };

    let events = discover_all(options);
    tokio::pin!(events);

    // In a real test environment with mock services:
    // - Verify RAOP services are discovered
    // - Verify TXT records are parsed correctly
    // - Verify device protocol is detected as RaopOnly
}

#[tokio::test]
async fn test_dual_protocol_device() {
    // Test device advertising both _raop._tcp and _airplay._tcp
    let options = DiscoveryOptions {
        discover_airplay2: true,
        discover_raop: true,
        timeout: Duration::from_secs(3),
        filter: None,
    };

    // In a real test:
    // - Verify both services are correlated to same device
    // - Verify protocol is DeviceProtocol::Both
    // - Verify both capability sets are populated
}
```

---

## Acceptance Criteria

- [ ] RAOP service type is correctly browsed via mDNS
- [ ] All TXT record fields are parsed correctly
- [ ] Codec list parsing handles all valid formats
- [ ] Encryption type detection is accurate
- [ ] Service name parsing extracts MAC and device name
- [ ] Device protocol detection distinguishes AirPlay 1/2/Both
- [ ] Unified discovery API returns correlated devices
- [ ] Password-protected devices are detected
- [ ] Missing TXT fields use sensible defaults
- [ ] All unit tests pass
- [ ] Integration tests with mock services pass

---

## Notes

- Some older devices may have non-standard TXT record formats
- MAC address in service name may not match actual network interface
- Consider caching discovered devices for quick reconnection
- Network changes should trigger re-discovery
- Some devices advertise both services with different capabilities
