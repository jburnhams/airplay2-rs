# Section 08: mDNS Discovery

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

AirPlay devices advertise themselves via mDNS (multicast DNS) / DNS-SD (Service Discovery). This section implements device discovery by browsing for the `_airplay._tcp.local.` service and parsing the TXT records to extract device capabilities.

## Objectives

- Implement continuous device discovery (streaming)
- Implement one-shot device scanning
- Parse TXT records for device capabilities
- Handle device appearance/disappearance events
- Deduplicate devices by ID

---

## Tasks

### 8.1 Discovery Module Structure

- [ ] **8.1.1** Define discovery module

**File:** `src/discovery/mod.rs`

```rust
//! mDNS device discovery for AirPlay devices

mod browser;
mod parser;

pub use browser::{DeviceBrowser, DiscoveryEvent};
pub use parser::parse_txt_records;

use crate::types::{AirPlayDevice, AirPlayConfig};
use crate::error::AirPlayError;
use futures::Stream;
use std::time::Duration;

/// Service type for AirPlay discovery
pub const AIRPLAY_SERVICE_TYPE: &str = "_airplay._tcp.local.";

/// Service type for AirPlay 2 RAOP (audio)
pub const RAOP_SERVICE_TYPE: &str = "_raop._tcp.local.";

/// Discover AirPlay devices continuously
///
/// Returns a stream that yields devices as they are discovered.
/// The stream continues until dropped.
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::discover;
/// use futures::StreamExt;
///
/// # async fn example() {
/// let mut devices = discover().await;
///
/// while let Some(event) = devices.next().await {
///     match event {
///         DiscoveryEvent::Added(device) => {
///             println!("Found: {}", device.name);
///         }
///         DiscoveryEvent::Removed(device_id) => {
///             println!("Lost: {}", device_id);
///         }
///     }
/// }
/// # }
/// ```
pub async fn discover() -> impl Stream<Item = DiscoveryEvent> {
    discover_with_config(&AirPlayConfig::default()).await
}

/// Discover devices with custom configuration
pub async fn discover_with_config(config: &AirPlayConfig) -> impl Stream<Item = DiscoveryEvent> {
    let browser = DeviceBrowser::new(config.clone());
    browser.browse()
}

/// Scan for devices with timeout
///
/// Performs a one-shot scan and returns all discovered devices.
///
/// # Arguments
///
/// * `timeout` - How long to scan for devices
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::scan;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// let devices = scan(Duration::from_secs(5)).await?;
///
/// for device in devices {
///     println!("{}: {}", device.name, device.address);
/// }
/// # Ok(())
/// # }
/// ```
pub async fn scan(timeout: Duration) -> Result<Vec<AirPlayDevice>, AirPlayError> {
    scan_with_config(timeout, &AirPlayConfig::default()).await
}

/// Scan for devices with custom configuration
pub async fn scan_with_config(
    timeout: Duration,
    config: &AirPlayConfig,
) -> Result<Vec<AirPlayDevice>, AirPlayError> {
    use futures::StreamExt;
    use std::collections::HashMap;

    let browser = DeviceBrowser::new(config.clone());
    let stream = browser.browse();

    let mut devices: HashMap<String, AirPlayDevice> = HashMap::new();

    // Use timeout
    let deadline = tokio::time::Instant::now() + timeout;

    tokio::pin!(stream);

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                break;
            }
            event = stream.next() => {
                match event {
                    Some(DiscoveryEvent::Added(device)) => {
                        devices.insert(device.id.clone(), device);
                    }
                    Some(DiscoveryEvent::Removed(id)) => {
                        devices.remove(&id);
                    }
                    Some(DiscoveryEvent::Updated(device)) => {
                        devices.insert(device.id.clone(), device);
                    }
                    None => break,
                }
            }
        }
    }

    Ok(devices.into_values().collect())
}
```

---

### 8.2 Device Browser

- [ ] **8.2.1** Implement the mDNS browser

**File:** `src/discovery/browser.rs`

```rust
use crate::types::{AirPlayDevice, AirPlayConfig, DeviceCapabilities};
use crate::error::AirPlayError;
use super::parser;
use futures::Stream;
use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Discovery events
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new device was discovered
    Added(AirPlayDevice),
    /// A device was removed/went offline
    Removed(String),
    /// Device information was updated
    Updated(AirPlayDevice),
}

/// mDNS browser for discovering AirPlay devices
pub struct DeviceBrowser {
    config: AirPlayConfig,
}

impl DeviceBrowser {
    /// Create a new device browser
    pub fn new(config: AirPlayConfig) -> Self {
        Self { config }
    }

    /// Start browsing for devices
    pub fn browse(self) -> impl Stream<Item = DiscoveryEvent> {
        DeviceBrowserStream::new(self.config)
    }
}

/// Stream implementation for device discovery
struct DeviceBrowserStream {
    config: AirPlayConfig,
    mdns: Option<mdns_sd::ServiceDaemon>,
    receiver: Option<mdns_sd::Receiver<mdns_sd::ServiceEvent>>,
    known_devices: HashMap<String, AirPlayDevice>,
}

impl DeviceBrowserStream {
    fn new(config: AirPlayConfig) -> Self {
        Self {
            config,
            mdns: None,
            receiver: None,
            known_devices: HashMap::new(),
        }
    }

    fn init(&mut self) -> Result<(), AirPlayError> {
        let mdns = mdns_sd::ServiceDaemon::new()
            .map_err(|e| AirPlayError::DiscoveryFailed {
                message: format!("Failed to create mDNS daemon: {}", e),
                source: None,
            })?;

        let receiver = mdns
            .browse(super::AIRPLAY_SERVICE_TYPE)
            .map_err(|e| AirPlayError::DiscoveryFailed {
                message: format!("Failed to browse: {}", e),
                source: None,
            })?;

        self.mdns = Some(mdns);
        self.receiver = Some(receiver);

        Ok(())
    }

    fn process_event(&mut self, event: mdns_sd::ServiceEvent) -> Option<DiscoveryEvent> {
        match event {
            mdns_sd::ServiceEvent::ServiceResolved(info) => {
                self.handle_resolved(info)
            }
            mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                self.handle_removed(&fullname)
            }
            _ => None,
        }
    }

    fn handle_resolved(&mut self, info: mdns_sd::ServiceInfo) -> Option<DiscoveryEvent> {
        // Extract device info from service
        let name = info.get_fullname().to_string();

        // Parse TXT records
        let txt_records: HashMap<String, String> = info
            .get_properties()
            .iter()
            .filter_map(|prop| {
                let key = prop.key().to_string();
                prop.val_str().map(|v| (key, v.to_string()))
            })
            .collect();

        // Get device ID from TXT records
        let device_id = txt_records
            .get("deviceid")
            .or_else(|| txt_records.get("pk"))
            .cloned()
            .unwrap_or_else(|| name.clone());

        // Parse capabilities from features flag
        let capabilities = txt_records
            .get("features")
            .and_then(|f| parser::parse_features(f))
            .unwrap_or_default();

        // Get first resolved address
        let address = info.get_addresses().iter().next().copied();

        let address = match address {
            Some(addr) => IpAddr::V4(addr),
            None => return None, // No address resolved yet
        };

        // Get friendly name
        let friendly_name = txt_records
            .get("model")
            .cloned()
            .or_else(|| {
                // Extract name from fullname (before first dot)
                name.split('.').next().map(|s| s.to_string())
            })
            .unwrap_or_else(|| "AirPlay Device".to_string());

        let device = AirPlayDevice {
            id: device_id.clone(),
            name: friendly_name,
            model: txt_records.get("model").cloned(),
            address,
            port: info.get_port(),
            capabilities,
            txt_records,
        };

        // Check if this is new or updated
        let event = if self.known_devices.contains_key(&device_id) {
            DiscoveryEvent::Updated(device.clone())
        } else {
            DiscoveryEvent::Added(device.clone())
        };

        self.known_devices.insert(device_id, device);

        Some(event)
    }

    fn handle_removed(&mut self, fullname: &str) -> Option<DiscoveryEvent> {
        // Find device by name
        let device_id = self
            .known_devices
            .iter()
            .find(|(_, d)| d.name == fullname || d.id == fullname)
            .map(|(id, _)| id.clone());

        if let Some(id) = device_id {
            self.known_devices.remove(&id);
            Some(DiscoveryEvent::Removed(id))
        } else {
            None
        }
    }
}

impl Stream for DeviceBrowserStream {
    type Item = DiscoveryEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Initialize on first poll
        if self.mdns.is_none() {
            if let Err(e) = self.init() {
                tracing::error!("Discovery init failed: {}", e);
                return Poll::Ready(None);
            }
        }

        // Try to receive from mdns
        let receiver = match &self.receiver {
            Some(r) => r,
            None => return Poll::Ready(None),
        };

        // Non-blocking receive
        match receiver.try_recv() {
            Ok(event) => {
                if let Some(discovery_event) = self.process_event(event) {
                    Poll::Ready(Some(discovery_event))
                } else {
                    // Event didn't produce a discovery event, wake and retry
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No events available, register waker and return pending
                // Note: mdns-sd doesn't support async natively, so we need to poll
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Poll::Ready(None)
            }
        }
    }
}

impl Drop for DeviceBrowserStream {
    fn drop(&mut self) {
        // Stop browsing
        if let Some(mdns) = self.mdns.take() {
            let _ = mdns.stop_browse(super::AIRPLAY_SERVICE_TYPE);
            let _ = mdns.shutdown();
        }
    }
}
```

---

### 8.3 TXT Record Parser

- [ ] **8.3.1** Implement TXT record parsing

**File:** `src/discovery/parser.rs`

```rust
//! Parser for AirPlay TXT record data

use crate::types::DeviceCapabilities;
use std::collections::HashMap;

/// Parse TXT records from mDNS response
pub fn parse_txt_records(records: &[String]) -> HashMap<String, String> {
    records
        .iter()
        .filter_map(|record| {
            let mut parts = record.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((key, value))
        })
        .collect()
}

/// Parse features flags from TXT record
///
/// The features value can be in hex format: "0x1234567890ABCDEF"
/// or comma-separated: "0x1234,0x5678"
pub fn parse_features(features_str: &str) -> Option<DeviceCapabilities> {
    let features = if features_str.contains(',') {
        // Comma-separated format: "0x1234,0x5678"
        // Combine into single 64-bit value
        let parts: Vec<&str> = features_str.split(',').collect();
        if parts.len() >= 2 {
            let hi = parse_hex(parts[0])?;
            let lo = parse_hex(parts[1])?;
            (hi << 32) | lo
        } else {
            parse_hex(parts[0])?
        }
    } else {
        // Single hex value
        parse_hex(features_str)?
    };

    Some(DeviceCapabilities::from_features(features))
}

/// Parse hex string to u64
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Parse device model from model string
pub fn parse_model_name(model: &str) -> &str {
    // Map internal model identifiers to friendly names
    match model {
        "AudioAccessory1,1" | "AudioAccessory1,2" => "HomePod",
        "AudioAccessory5,1" => "HomePod mini",
        "AppleTV3,1" | "AppleTV3,2" => "Apple TV (3rd generation)",
        "AppleTV5,3" => "Apple TV (4th generation)",
        "AppleTV6,2" => "Apple TV 4K",
        "AppleTV11,1" => "Apple TV 4K (2nd generation)",
        "AppleTV14,1" => "Apple TV 4K (3rd generation)",
        "AirPort10,1" => "AirPort Express",
        _ => model,
    }
}

/// Known TXT record keys for AirPlay
pub mod txt_keys {
    /// Device ID (MAC address format)
    pub const DEVICE_ID: &str = "deviceid";
    /// Features bitmask
    pub const FEATURES: &str = "features";
    /// Flags
    pub const FLAGS: &str = "flags";
    /// Model identifier
    pub const MODEL: &str = "model";
    /// Protocol version
    pub const PROTOCOL_VERSION: &str = "protovers";
    /// Source version
    pub const SOURCE_VERSION: &str = "srcvers";
    /// Volume (0-1)
    pub const VOLUME: &str = "vv";
    /// Public key (for pairing)
    pub const PUBLIC_KEY: &str = "pk";
    /// Password required
    pub const PASSWORD: &str = "pw";
    /// PIN required
    pub const PIN: &str = "pin";
    /// Group contains discoverable leader
    pub const GROUP_CONTAINS_LEADER: &str = "gcgl";
    /// Group UUID
    pub const GROUP_UUID: &str = "gid";
    /// Is group leader
    pub const IS_GROUP_LEADER: &str = "igl";
    /// AirPlay version
    pub const AIRPLAY_VERSION: &str = "am";
}

/// AirPlay feature bits
///
/// Reference: https://emanuelecozzi.net/docs/airplay2/features
pub mod feature_bits {
    /// Video supported
    pub const VIDEO: u64 = 1 << 0;
    /// Photo supported
    pub const PHOTO: u64 = 1 << 1;
    /// Video fade in
    pub const VIDEO_FADE_IN: u64 = 1 << 2;
    /// Video HTTP live streaming
    pub const VIDEO_HLS: u64 = 1 << 3;
    /// Slideshow supported
    pub const SLIDESHOW: u64 = 1 << 5;
    /// Screen mirroring
    pub const SCREEN: u64 = 1 << 7;
    /// Screen rotation
    pub const SCREEN_ROTATE: u64 = 1 << 8;
    /// Audio supported
    pub const AUDIO: u64 = 1 << 9;
    /// Audio redundant
    pub const AUDIO_REDUNDANT: u64 = 1 << 11;
    /// FairPlay secure auth
    pub const FPS_AP_V2_5: u64 = 1 << 12;
    /// Photo caching
    pub const PHOTO_CACHING: u64 = 1 << 13;
    /// Authentication type 4
    pub const AUTH_TYPE_4: u64 = 1 << 14;
    /// Metadata type 1
    pub const METADATA_TYPE_1: u64 = 1 << 15;
    /// Metadata type 2
    pub const METADATA_TYPE_2: u64 = 1 << 16;
    /// Metadata type 3 (artwork)
    pub const METADATA_TYPE_3: u64 = 1 << 17;
    /// Audio format 1
    pub const AUDIO_FORMAT_1: u64 = 1 << 18;
    /// Audio format 2
    pub const AUDIO_FORMAT_2: u64 = 1 << 19;
    /// Audio format 3
    pub const AUDIO_FORMAT_3: u64 = 1 << 20;
    /// Audio format 4
    pub const AUDIO_FORMAT_4: u64 = 1 << 21;
    /// Authentication type 1
    pub const AUTH_TYPE_1: u64 = 1 << 23;
    /// Authentication type 8
    pub const AUTH_TYPE_8: u64 = 1 << 26;
    /// Supports legacy pairing
    pub const LEGACY_PAIRING: u64 = 1 << 27;
    /// RAOP supported
    pub const RAOP: u64 = 1 << 28;
    /// Is carplay
    pub const IS_CARPLAY: u64 = 1 << 29;
    /// Control channel encryption
    pub const CONTROL_CHANNEL_ENCRYPT: u64 = 1 << 30;
    /// Supports unified media control
    pub const UNIFIED_MEDIA_CONTROL: u64 = 1 << 32;
    /// Supports buffered audio
    pub const BUFFERED_AUDIO: u64 = 1 << 38;
    /// Supports PTP clock
    pub const PTP_CLOCK: u64 = 1 << 40;
    /// Screen multi codec
    pub const SCREEN_MULTI_CODEC: u64 = 1 << 41;
    /// System pairing
    pub const SYSTEM_PAIRING: u64 = 1 << 43;
    /// Supports AirPlay 2 / APv2.5
    pub const AIRPLAY_2: u64 = 1 << 48;
    /// Supports system authentication
    pub const SYSTEM_AUTH: u64 = 1 << 49;
    /// Supports CoreUtils pairing and encryption
    pub const COREUTILS_PAIRING: u64 = 1 << 51;
    /// Supports transient pairing
    pub const TRANSIENT_PAIRING: u64 = 1 << 52;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_simple() {
        assert_eq!(parse_hex("0x1234"), Some(0x1234));
        assert_eq!(parse_hex("1234"), Some(0x1234));
        assert_eq!(parse_hex("0X1234"), Some(0x1234));
    }

    #[test]
    fn test_parse_features_single() {
        let caps = parse_features("0x1C340405F8A00").unwrap();
        assert!(caps.supports_audio);
    }

    #[test]
    fn test_parse_features_comma() {
        let caps = parse_features("0x1C340,0x405F8A00").unwrap();
        // Check that features from both parts are combined
        assert!(caps.raw_features != 0);
    }

    #[test]
    fn test_parse_model_name() {
        assert_eq!(parse_model_name("AudioAccessory5,1"), "HomePod mini");
        assert_eq!(parse_model_name("Unknown"), "Unknown");
    }
}
```

---

## Unit Tests

### Test File: `src/discovery/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_with_timeout() {
        // This test requires network access
        // In CI, it will likely return empty results
        let result = scan(Duration::from_millis(100)).await;
        assert!(result.is_ok());
    }
}
```

### Test File: `src/discovery/parser.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_txt_records() {
        let records = vec![
            "key1=value1".to_string(),
            "key2=value2".to_string(),
            "key3=".to_string(),
        ];

        let parsed = parse_txt_records(&records);

        assert_eq!(parsed.get("key1"), Some(&"value1".to_string()));
        assert_eq!(parsed.get("key2"), Some(&"value2".to_string()));
        assert_eq!(parsed.get("key3"), Some(&"".to_string()));
    }

    #[test]
    fn test_feature_bit_audio() {
        let features = feature_bits::AUDIO;
        let caps = DeviceCapabilities::from_features(features);
        assert!(caps.supports_audio);
    }

    #[test]
    fn test_feature_bit_airplay2() {
        let features = feature_bits::AIRPLAY_2 | feature_bits::AUDIO;
        let caps = DeviceCapabilities::from_features(features);
        assert!(caps.airplay2);
        assert!(caps.supports_audio);
    }

    #[test]
    fn test_feature_bit_grouping() {
        let features = feature_bits::UNIFIED_MEDIA_CONTROL;
        let caps = DeviceCapabilities::from_features(features);
        assert!(caps.supports_grouping);
    }
}
```

---

## Integration Tests

### Test: Real device discovery (manual)

```rust
// tests/integration/discovery_tests.rs

#[tokio::test]
#[ignore] // Run manually with `cargo test -- --ignored`
async fn test_discover_real_devices() {
    use airplay2::scan;
    use std::time::Duration;

    let devices = scan(Duration::from_secs(5)).await.unwrap();

    println!("Found {} devices:", devices.len());
    for device in &devices {
        println!("  - {} ({})", device.name, device.id);
        println!("    Address: {}:{}", device.address, device.port);
        println!("    Model: {:?}", device.model);
        println!("    AirPlay 2: {}", device.supports_airplay2());
        println!("    Grouping: {}", device.supports_grouping());
    }

    // At least verify we can run without crashing
}
```

---

## Acceptance Criteria

- [ ] `discover()` returns a stream of device events
- [ ] `scan()` returns a list of devices within timeout
- [ ] TXT records are parsed correctly
- [ ] Features flags are parsed (both formats)
- [ ] Device capabilities are derived from features
- [ ] Device appearance/removal is detected
- [ ] Multiple addresses per device are handled
- [ ] All unit tests pass
- [ ] Integration test works with real devices (manual)

---

## Notes

- The `mdns-sd` crate doesn't support true async, so we poll the receiver
- Consider adding IPv6 support
- Some devices may use RAOP service type instead of AirPlay
- Features parsing may need updates as Apple adds new capabilities
- Consider caching resolved devices to reduce network traffic
