# Section 02: Core Types, Errors & Configuration

**VERIFIED**: AirPlayDevice (addresses, raop fields, methods), QueueItemId/QueueItem, RAOP types exports, RaopError enum checked against source.

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)

## Overview

This section defines the foundational types used throughout the library: device representations, track information, playback state, configuration, and error handling. These types form the public API contract and are used by all other sections.

## Objectives

- Define all public-facing data structures
- Implement comprehensive error types
- Create configuration system
- Ensure all types are `Send + Sync` where appropriate
- Provide builder patterns for complex types

---

## Tasks

### 2.1 Device Types

- [x] **2.1.1** Implement `AirPlayDevice` struct

**File:** `src/types/device.rs`

```rust
use super::raop::RaopCapabilities;
use std::net::IpAddr;
use std::collections::HashMap;

/// Represents a discovered AirPlay 2 device on the network
#[derive(Debug, Clone, PartialEq)]
pub struct AirPlayDevice {
    /// Unique device identifier (from TXT record)
    pub id: String,

    /// Human-readable device name (e.g., "Living Room HomePod")
    pub name: String,

    /// Device model identifier (e.g., "AudioAccessory5,1" for HomePod Mini)
    pub model: Option<String>,

    /// Resolved IP addresses
    pub addresses: Vec<IpAddr>,

    /// AirPlay service port
    pub port: u16,

    /// Device capabilities parsed from features flags
    pub capabilities: DeviceCapabilities,

    /// RAOP (AirPlay 1) service port
    pub raop_port: Option<u16>,

    /// RAOP capabilities parsed from TXT records
    pub raop_capabilities: Option<RaopCapabilities>,

    /// Raw TXT record data for protocol use
    pub txt_records: HashMap<String, String>,
}

/// Device capability flags parsed from AirPlay features
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceCapabilities {
    /// Supports AirPlay 2 protocol
    pub airplay2: bool,

    /// Supports multi-room/grouped playback
    pub supports_grouping: bool,

    /// Supports screen mirroring (not used, for info only)
    pub supports_screen: bool,

    /// Supports audio streaming
    pub supports_audio: bool,

    /// Supports high-resolution audio
    pub supports_hires_audio: bool,

    /// Supports buffered audio (for gapless playback)
    pub supports_buffered_audio: bool,

    /// Supports persistent pairing
    pub supports_persistent_pairing: bool,

    /// Supports HomeKit pairing
    pub supports_homekit_pairing: bool,

    /// Supports transient pairing
    pub supports_transient_pairing: bool,

    /// Raw features bitmask
    pub raw_features: u64,
}

impl AirPlayDevice {
    /// Check if this device supports AirPlay 2 features
    pub fn supports_airplay2(&self) -> bool {
        self.capabilities.airplay2
    }

    /// Check if this device supports RAOP (AirPlay 1)
    pub fn supports_raop(&self) -> bool {
        self.raop_port.is_some()
    }

    /// Check if this device can be part of a multi-room group
    pub fn supports_grouping(&self) -> bool {
        self.capabilities.supports_grouping
    }

    /// Get device volume if available from discovery
    pub fn discovered_volume(&self) -> Option<f32> {
        self.txt_records
            .get("vv")
            .and_then(|v| v.parse().ok())
    }

    /// Get the primary IP address (prefers IPv4 for better connectivity)
    pub fn address(&self) -> IpAddr {
        self.addresses
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| self.addresses.first())
            .copied()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    }
}
```

- [x] **2.1.2** Implement `DeviceCapabilities` parsing from features flags

**Method in:** `src/types/device.rs`

```rust
impl DeviceCapabilities {
    /// Parse capabilities from AirPlay features bitmask
    ///
    /// Features are documented at:
    /// https://emanuelecozzi.net/docs/airplay2/features
    pub fn from_features(features: u64) -> Self {
        Self {
            // Bit 9: Audio
            supports_audio: (features & (1 << 9)) != 0,
            // Bit 38: Supports buffered audio
            supports_buffered_audio: (features & (1 << 38)) != 0,
            // Bit 48: Supports AirPlay 2 / MFi authentication
            airplay2: (features & (1 << 48)) != 0,
            // Bit 32: Supports unified media control
            supports_grouping: (features & (1 << 32)) != 0,
            // Add other capability parsing...
            raw_features: features,
            ..Default::default()
        }
    }
}
```

---

### 2.2 Track and Queue Types

- [x] **2.2.1** Implement `TrackInfo` struct

**File:** `src/types/track.rs`

```rust
/// Information about a track for playback
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    /// URL to audio content (HTTP/HTTPS)
    pub url: String,

    /// Track title
    pub title: String,

    /// Artist name
    pub artist: String,

    /// Album name
    pub album: Option<String>,

    /// URL to album artwork
    pub artwork_url: Option<String>,

    /// Track duration in seconds
    pub duration_secs: Option<f64>,

    /// Track number on album
    pub track_number: Option<u32>,

    /// Disc number
    pub disc_number: Option<u32>,

    /// Genre
    pub genre: Option<String>,

    /// Content identifier for queue management
    pub content_id: Option<String>,
}

impl TrackInfo {
    /// Create a new TrackInfo with required fields
    pub fn new(url: impl Into<String>, title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    /// Builder method to set album
    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    /// Builder method to set artwork URL
    pub fn with_artwork(mut self, artwork_url: impl Into<String>) -> Self {
        self.artwork_url = Some(artwork_url.into());
        self
    }

    /// Builder method to set duration
    pub fn with_duration(mut self, duration_secs: f64) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }
}
```

- [x] **2.2.2** Implement `QueueItemId` and `QueueItem` for internal queue tracking

**File:** `src/types/track.rs`

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a queue item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueueItemId(pub u64);

impl QueueItemId {
    /// Generate a new unique ID
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for QueueItemId {
    fn default() -> Self {
        Self::new()
    }
}

/// A track in the playback queue with unique identifier
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// Unique identifier for this queue position
    pub id: QueueItemId,

    /// Track information
    pub track: TrackInfo,

    /// Original position (before shuffle)
    pub original_position: usize,
}

impl QueueItem {
    /// Create a new queue item
    pub fn new(track: TrackInfo, position: usize) -> Self {
        Self {
            id: QueueItemId::new(),
            track,
            original_position: position,
        }
    }
}
```

---

### 2.3 Playback State Types

- [x] **2.3.1** Implement `PlaybackState` struct

**File:** `src/types/state.rs`

```rust
use super::track::{TrackInfo, QueueItem};

/// Current playback state of a connected device
#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    /// Whether audio is currently playing
    pub is_playing: bool,

    /// Current track info (None if queue empty)
    pub current_track: Option<TrackInfo>,

    /// Position in current track (seconds)
    pub position_secs: f64,

    /// Duration of current track (seconds)
    pub duration_secs: Option<f64>,

    /// Current volume (0.0 - 1.0)
    pub volume: f32,

    /// Current queue
    pub queue: Vec<QueueItem>,

    /// Index of current track in queue
    pub queue_index: Option<usize>,

    /// Whether shuffle is enabled
    pub shuffle: bool,

    /// Current repeat mode
    pub repeat: RepeatMode,

    /// Connection state
    pub connection_state: ConnectionState,
}

/// Repeat mode for queue playback
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepeatMode {
    /// No repeat
    #[default]
    Off,
    /// Repeat entire queue
    All,
    /// Repeat current track
    One,
}

/// Connection state of the client
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// Connection in progress
    Connecting,
    /// Pairing/authenticating
    Pairing,
    /// Connected and ready
    Connected,
    /// Connection lost, attempting reconnect
    Reconnecting,
}
```

- [x] **2.3.2** Implement `PlaybackInfo` for high-level API

**File:** `src/types/state.rs`

```rust
use super::track::TrackInfo;

/// Playback info matching music-player integration requirements
#[derive(Debug, Clone, Default)]
pub struct PlaybackInfo {
    /// Currently playing track
    pub current_track: Option<TrackInfo>,

    /// Index in queue
    pub index: u32,

    /// Position in milliseconds
    pub position_ms: u32,

    /// Whether currently playing
    pub is_playing: bool,

    /// Queue items with unique IDs: (track, item_id)
    pub items: Vec<(TrackInfo, i32)>,
}

impl From<&PlaybackState> for PlaybackInfo {
    fn from(state: &PlaybackState) -> Self {
        Self {
            current_track: state.current_track.clone(),
            index: state.queue_index
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(0),
            position_ms: (state.position_secs * 1000.0) as u32,
            is_playing: state.is_playing,
            items: state
                .queue
                .iter()
                .map(|item| (item.track.clone(), item.id.0 as i32))
                .collect(),
        }
    }
}
```

---

### 2.4 Configuration

- [x] **2.4.1** Implement `AirPlayConfig` struct

**File:** `src/types/config.rs`

```rust
use std::time::Duration;

/// Configuration for AirPlay client behavior
#[derive(Debug, Clone)]
pub struct AirPlayConfig {
    /// Timeout for device discovery scan (default: 5 seconds)
    pub discovery_timeout: Duration,

    /// Timeout for connection attempts (default: 10 seconds)
    pub connection_timeout: Duration,

    /// Interval for polling playback state (default: 500ms)
    pub state_poll_interval: Duration,

    /// Enable debug logging of protocol messages
    pub debug_protocol: bool,

    /// Number of reconnection attempts (default: 3)
    pub reconnect_attempts: u32,

    /// Delay between reconnection attempts (default: 1 second)
    pub reconnect_delay: Duration,

    /// Audio buffer size in frames (default: 44100 = 1 second at 44.1kHz)
    pub audio_buffer_frames: usize,

    /// Path to store persistent pairing keys (None = transient only)
    pub pairing_storage_path: Option<std::path::PathBuf>,
}

impl Default for AirPlayConfig {
    fn default() -> Self {
        Self {
            discovery_timeout: Duration::from_secs(5),
            connection_timeout: Duration::from_secs(10),
            state_poll_interval: Duration::from_millis(500),
            debug_protocol: false,
            reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(1),
            audio_buffer_frames: 44100,
            pairing_storage_path: None,
        }
    }
}

impl AirPlayConfig {
    /// Create a new config builder
    pub fn builder() -> AirPlayConfigBuilder {
        AirPlayConfigBuilder::default()
    }
}
```

- [x] **2.4.2** Implement `AirPlayConfigBuilder`

**File:** `src/types/config.rs`

```rust
/// Builder for AirPlayConfig
#[derive(Debug, Clone, Default)]
pub struct AirPlayConfigBuilder {
    config: AirPlayConfig,
}

impl AirPlayConfigBuilder {
    /// Set discovery timeout
    pub fn discovery_timeout(mut self, timeout: Duration) -> Self {
        self.config.discovery_timeout = timeout;
        self
    }

    /// Set connection timeout
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Set state polling interval
    pub fn state_poll_interval(mut self, interval: Duration) -> Self {
        self.config.state_poll_interval = interval;
        self
    }

    /// Enable protocol debug logging
    pub fn debug_protocol(mut self, enable: bool) -> Self {
        self.config.debug_protocol = enable;
        self
    }

    /// Set pairing storage path for persistent pairing
    pub fn pairing_storage(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config.pairing_storage_path = Some(path.into());
        self
    }

    /// Build the configuration
    pub fn build(self) -> AirPlayConfig {
        self.config
    }
}
```

---

### 2.5 Error Types

- [x] **2.5.1** Implement comprehensive `AirPlayError` enum

**File:** `src/error.rs`

```rust
use std::io;
use thiserror::Error;

/// RAOP-specific errors
#[derive(Debug, Error)]
pub enum RaopError {
    /// RSA authentication failed
    #[error("RSA authentication failed")]
    AuthenticationFailed,

    /// Unsupported encryption type
    #[error("unsupported encryption type: {0}")]
    UnsupportedEncryption(String),

    /// SDP parsing error
    #[error("SDP parsing error: {0}")]
    SdpParseError(String),

    /// Key exchange failed
    #[error("key exchange failed: {0}")]
    KeyExchangeFailed(String),

    /// Audio encryption error
    #[error("audio encryption error: {0}")]
    EncryptionError(String),

    /// Timing synchronization failed
    #[error("timing sync failed")]
    TimingSyncFailed,

    /// Retransmit buffer overflow
    #[error("retransmit buffer overflow")]
    RetransmitBufferOverflow,
}

/// Errors that can occur during AirPlay operations
#[derive(Debug, Error)]
pub enum AirPlayError {
    /// RAOP error
    #[error("RAOP error: {0}")]
    Raop(#[from] RaopError),

    // ===== Discovery Errors =====

    /// Device was not found during discovery
    #[error("device not found: {device_id}")]
    DeviceNotFound {
        device_id: String,
    },

    /// mDNS discovery failed
    #[error("discovery failed: {message}")]
    DiscoveryFailed {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    // ===== Connection Errors =====

    /// Failed to establish connection to device
    #[error("connection failed to {device_name}: {message}")]
    ConnectionFailed {
        device_name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Connection was closed unexpectedly
    #[error("device disconnected: {device_name}")]
    Disconnected {
        device_name: String,
    },

    /// Connection timed out
    #[error("connection timeout after {duration:?}")]
    ConnectionTimeout {
        duration: std::time::Duration,
    },

    // ===== Authentication Errors =====

    /// Pairing/authentication failed
    #[error("authentication failed: {message}")]
    AuthenticationFailed {
        message: String,
        /// Whether the error is recoverable by retrying
        recoverable: bool,
    },

    /// Pairing required but not initiated
    #[error("pairing required with device {device_name}")]
    PairingRequired {
        device_name: String,
    },

    /// Stored pairing keys are invalid or expired
    #[error("pairing keys invalid for device {device_id}")]
    PairingInvalid {
        device_id: String,
    },

    // ===== Protocol Errors =====

    /// RTSP protocol error
    #[error("RTSP error: {message}")]
    RtspError {
        message: String,
        status_code: Option<u16>,
    },

    /// RTP protocol error
    #[error("RTP error: {message}")]
    RtpError {
        message: String,
    },

    /// Unexpected protocol response
    #[error("unexpected response: expected {expected}, got {actual}")]
    UnexpectedResponse {
        expected: String,
        actual: String,
    },

    /// Protocol message encoding/decoding failed
    #[error("codec error: {message}")]
    CodecError {
        message: String,
    },

    // ===== Playback Errors =====

    /// Playback error from device
    #[error("playback error: {message}")]
    PlaybackError {
        message: String,
    },

    /// Invalid URL for streaming
    #[error("invalid URL: {url} - {reason}")]
    InvalidUrl {
        url: String,
        reason: String,
    },

    /// Audio format not supported
    #[error("unsupported audio format: {format}")]
    UnsupportedFormat {
        format: String,
    },

    /// Queue operation failed
    #[error("queue error: {message}")]
    QueueError {
        message: String,
    },

    /// Seek position out of range
    #[error("seek position {position} out of range (duration: {duration:?})")]
    SeekOutOfRange {
        position: f64,
        duration: Option<f64>,
    },

    // ===== I/O Errors =====

    /// Network I/O error
    #[error("network error: {0}")]
    NetworkError(#[from] io::Error),

    /// Operation timed out
    #[error("operation timed out")]
    Timeout,

    // ===== State Errors =====

    /// Operation not valid in current state
    #[error("invalid state: {message}")]
    InvalidState {
        message: String,
        current_state: String,
    },

    /// Device is busy with another operation
    #[error("device busy")]
    DeviceBusy,

    // ===== Internal Errors =====

    /// Internal library error
    #[error("internal error: {message}")]
    InternalError {
        message: String,
    },

    /// Feature not yet implemented
    #[error("not implemented: {feature}")]
    NotImplemented {
        feature: String,
    },

    /// Invalid parameter provided
    #[error("invalid parameter: {name} - {message}")]
    InvalidParameter {
        name: String,
        message: String,
    },

    /// General I/O error
    #[error("I/O error: {message}")]
    IoError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl AirPlayError {
    /// Check if this error is recoverable by retrying
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            AirPlayError::ConnectionTimeout { .. }
                | AirPlayError::Timeout
                | AirPlayError::NetworkError(_)
                | AirPlayError::DeviceBusy
        )
    }

    /// Check if this error indicates connection loss
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            AirPlayError::Disconnected { .. }
                | AirPlayError::ConnectionFailed { .. }
                | AirPlayError::ConnectionTimeout { .. }
        )
    }
}

/// Result type alias for AirPlay operations
pub type Result<T> = std::result::Result<T, AirPlayError>;
```

---

### 2.6 Module Organization

- [x] **2.6.1** Create types module entry point

**File:** `src/types/mod.rs`

```rust
//! Core types for the airplay2 library

mod config;
mod device;
/// RAOP (AirPlay 1) types
pub mod raop;
mod state;
mod track;

pub use config::{AirPlayConfig, AirPlayConfigBuilder};
pub use device::{AirPlayDevice, DeviceCapabilities};
pub use raop::{RaopCapabilities, RaopCodec, RaopEncryption, RaopMetadataType};
pub use state::{ConnectionState, PlaybackInfo, PlaybackState, RepeatMode};
pub use track::{QueueItem, QueueItemId, TrackInfo};
```

---

## Unit Tests

### Test File: `src/types/device.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_capabilities_from_features() {
        // Test known HomePod Mini features value
        let features = 0x1C340405F8A00;
        let caps = DeviceCapabilities::from_features(features);

        assert!(caps.supports_audio);
        assert!(caps.airplay2);
    }

    #[test]
    fn test_device_capabilities_empty() {
        let caps = DeviceCapabilities::from_features(0);

        assert!(!caps.supports_audio);
        assert!(!caps.airplay2);
    }

    #[test]
    fn test_device_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AirPlayDevice>();
    }
}
```

### Test File: `src/types/track.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_info_builder() {
        let track = TrackInfo::new(
            "http://example.com/track.mp3",
            "Test Track",
            "Test Artist",
        )
        .with_album("Test Album")
        .with_duration(180.5);

        assert_eq!(track.title, "Test Track");
        assert_eq!(track.album, Some("Test Album".to_string()));
        assert_eq!(track.duration_secs, Some(180.5));
    }

    #[test]
    fn test_track_info_default() {
        let track = TrackInfo::default();
        assert!(track.url.is_empty());
        assert!(track.duration_secs.is_none());
    }

    #[test]
    fn test_track_info_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TrackInfo>();
    }
}
```

### Test File: `src/types/state.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_state_default() {
        let state = PlaybackState::default();
        assert!(!state.is_playing);
        assert!(state.current_track.is_none());
        assert_eq!(state.volume, 0.0);
        assert_eq!(state.repeat, RepeatMode::Off);
    }

    #[test]
    fn test_playback_info_from_state() {
        let mut state = PlaybackState::default();
        state.position_secs = 30.5;
        state.is_playing = true;

        let info = PlaybackInfo::from(&state);

        assert_eq!(info.position_ms, 30500);
        assert!(info.is_playing);
    }

    #[test]
    fn test_repeat_mode_equality() {
        assert_eq!(RepeatMode::Off, RepeatMode::Off);
        assert_ne!(RepeatMode::Off, RepeatMode::All);
    }
}
```

### Test File: `src/types/config.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AirPlayConfig::default();

        assert_eq!(config.discovery_timeout, Duration::from_secs(5));
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
        assert!(!config.debug_protocol);
    }

    #[test]
    fn test_config_builder() {
        let config = AirPlayConfig::builder()
            .discovery_timeout(Duration::from_secs(10))
            .debug_protocol(true)
            .build();

        assert_eq!(config.discovery_timeout, Duration::from_secs(10));
        assert!(config.debug_protocol);
    }
}
```

### Test File: `src/error.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AirPlayError::DeviceNotFound {
            device_id: "ABC123".to_string(),
        };
        assert_eq!(err.to_string(), "device not found: ABC123");
    }

    #[test]
    fn test_error_is_recoverable() {
        assert!(AirPlayError::Timeout.is_recoverable());
        assert!(AirPlayError::DeviceBusy.is_recoverable());

        let auth_err = AirPlayError::AuthenticationFailed {
            message: "bad pin".to_string(),
            recoverable: false,
        };
        assert!(!auth_err.is_recoverable());
    }

    #[test]
    fn test_error_is_connection_lost() {
        let err = AirPlayError::Disconnected {
            device_name: "HomePod".to_string(),
        };
        assert!(err.is_connection_lost());
        assert!(!AirPlayError::Timeout.is_connection_lost());
    }

    #[test]
    fn test_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        let err: AirPlayError = io_err.into();

        assert!(matches!(err, AirPlayError::NetworkError(_)));
    }

    #[test]
    fn test_error_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AirPlayError>();
    }
}
```

---

## Integration Tests

### Test: Types serialize/deserialize correctly with plist (future)

```rust
// tests/integration/types_tests.rs

#[test]
fn test_track_info_roundtrip() {
    // Will be tested when plist codec is implemented
    // Ensure TrackInfo can be serialized to plist and back
}
```

---

## Acceptance Criteria

- [x] All type structs compile and have proper derives
- [x] All types that should be `Send + Sync` are verified
- [x] `AirPlayError` covers all known error cases
- [x] `AirPlayError::is_recoverable()` correctly identifies retryable errors
- [x] `AirPlayConfig` has sensible defaults
- [x] Builder patterns work correctly
- [x] All unit tests pass
- [x] Documentation examples compile
- [x] `cargo doc` generates clean documentation for all types

---

## Notes

- Feature flags parsing needs to be verified against real device TXT records
- Consider adding `serde` derives behind a feature flag for JSON debugging
- The `content_id` field in TrackInfo may need refinement based on protocol analysis
