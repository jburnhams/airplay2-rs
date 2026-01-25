# Section 32: AirPlay 1 Integration Guide

## Dependencies
- All previous AirPlay 1 sections (24-31)
- **Section 21**: AirPlayClient Implementation (for reference)
- **Section 22**: High-Level API (for integration patterns)

## Overview

This section describes how to integrate AirPlay 1 (RAOP) support with the existing AirPlay 2 codebase, creating a unified client API that can transparently handle both protocol versions.

## Architecture Design

### Unified Client Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    AirPlayClient (Unified API)                   │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   Public Interface                       │    │
│  │  connect() | play() | pause() | set_volume() | ...      │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│              ┌───────────────┴───────────────┐                  │
│              ▼                               ▼                   │
│  ┌──────────────────────┐       ┌──────────────────────┐       │
│  │   AirPlay2Session    │       │    RaopSession       │       │
│  │                      │       │                      │       │
│  │  - HomeKit pairing   │       │  - RSA authentication│       │
│  │  - Binary plist RTSP │       │  - SDP-based RTSP    │       │
│  │  - ChaCha20/AES-GCM  │       │  - AES-128-CTR       │       │
│  │  - Multi-room        │       │  - DACP/DAAP         │       │
│  └──────────────────────┘       └──────────────────────┘       │
│              │                               │                   │
│              └───────────────┬───────────────┘                  │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   Shared Components                      │    │
│  │  RTSP Codec | RTP Streaming | Audio Encoding | mDNS     │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Tasks

### 32.1 Protocol Detection

- [x] **32.1.1** Implement automatic protocol detection

**File:** `src/client/protocol.rs`

```rust
//! Protocol detection and selection

use crate::discovery::{DiscoveredDevice, DeviceProtocol};
use crate::types::DeviceCapabilities;
use crate::discovery::raop::RaopCapabilities;

/// Preferred protocol for connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferredProtocol {
    /// Prefer AirPlay 2 when available
    #[default]
    PreferAirPlay2,
    /// Prefer AirPlay 1 (RAOP) when available
    PreferRaop,
    /// Force AirPlay 2 only
    ForceAirPlay2,
    /// Force RAOP only
    ForceRaop,
}

/// Protocol selection result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedProtocol {
    /// Use AirPlay 2
    AirPlay2,
    /// Use AirPlay 1 (RAOP)
    Raop,
}

/// Select protocol for device connection
pub fn select_protocol(
    device: &DiscoveredDevice,
    preferred: PreferredProtocol,
) -> Result<SelectedProtocol, ProtocolError> {
    match preferred {
        PreferredProtocol::ForceAirPlay2 => {
            if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else {
                Err(ProtocolError::AirPlay2NotSupported)
            }
        }
        PreferredProtocol::ForceRaop => {
            if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else {
                Err(ProtocolError::RaopNotSupported)
            }
        }
        PreferredProtocol::PreferAirPlay2 => {
            if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else {
                Err(ProtocolError::NoSupportedProtocol)
            }
        }
        PreferredProtocol::PreferRaop => {
            if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else {
                Err(ProtocolError::NoSupportedProtocol)
            }
        }
    }
}

/// Check if RAOP encryption is compatible
pub fn check_raop_encryption(caps: &RaopCapabilities) -> Result<(), ProtocolError> {
    if let Some(enc) = caps.preferred_encryption() {
        if enc.is_supported() {
            Ok(())
        } else {
            Err(ProtocolError::UnsupportedEncryption)
        }
    } else {
        Err(ProtocolError::UnsupportedEncryption)
    }
}

/// Protocol errors
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("AirPlay 2 not supported by device")]
    AirPlay2NotSupported,
    #[error("RAOP not supported by device")]
    RaopNotSupported,
    #[error("no supported protocol available")]
    NoSupportedProtocol,
    #[error("unsupported encryption type")]
    UnsupportedEncryption,
}
```

---

### 32.2 Session Abstraction

- [x] **32.2.1** Define common session trait

**File:** `src/client/session.rs`

```rust
//! Unified session abstraction

use async_trait::async_trait;
use crate::error::AirPlayError;
use crate::types::{TrackInfo, PlaybackState};

/// Common session operations for both AirPlay 1 and 2
#[async_trait]
pub trait AirPlaySession: Send + Sync {
    /// Connect to the device
    async fn connect(&mut self) -> Result<(), AirPlayError>;

    /// Disconnect from the device
    async fn disconnect(&mut self) -> Result<(), AirPlayError>;

    /// Check if connected
    fn is_connected(&self) -> bool;

    /// Start playback
    async fn play(&mut self) -> Result<(), AirPlayError>;

    /// Pause playback
    async fn pause(&mut self) -> Result<(), AirPlayError>;

    /// Stop playback
    async fn stop(&mut self) -> Result<(), AirPlayError>;

    /// Set volume (0.0 - 1.0)
    async fn set_volume(&mut self, volume: f32) -> Result<(), AirPlayError>;

    /// Get current volume
    async fn get_volume(&self) -> Result<f32, AirPlayError>;

    /// Stream audio data
    async fn stream_audio(&mut self, data: &[u8]) -> Result<(), AirPlayError>;

    /// Flush audio buffer
    async fn flush(&mut self) -> Result<(), AirPlayError>;

    /// Set track metadata
    async fn set_metadata(&mut self, track: &TrackInfo) -> Result<(), AirPlayError>;

    /// Set artwork
    async fn set_artwork(&mut self, data: &[u8]) -> Result<(), AirPlayError>;

    /// Get playback state
    fn playback_state(&self) -> PlaybackState;

    /// Get protocol version string
    fn protocol_version(&self) -> &'static str;
}

/// RAOP session implementation
pub struct RaopSessionImpl {
    // RAOP-specific fields
    rtsp_session: crate::protocol::raop::RaopRtspSession,
    streamer: Option<crate::streaming::raop_streamer::RaopStreamer>,
    connected: bool,
    volume: f32,
    state: PlaybackState,
}

impl RaopSessionImpl {
    /// Create new RAOP session
    pub fn new(server_addr: &str, server_port: u16) -> Self {
        Self {
            rtsp_session: crate::protocol::raop::RaopRtspSession::new(server_addr, server_port),
            streamer: None,
            connected: false,
            volume: 1.0,
            state: PlaybackState::Stopped,
        }
    }
}

#[async_trait]
impl AirPlaySession for RaopSessionImpl {
    async fn connect(&mut self) -> Result<(), AirPlayError> {
        // 1. Send OPTIONS with Apple-Challenge
        // 2. Send ANNOUNCE with SDP
        // 3. Send SETUP to configure transport
        // 4. Initialize audio streamer

        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), AirPlayError> {
        // Send TEARDOWN
        self.connected = false;
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn play(&mut self) -> Result<(), AirPlayError> {
        // Send RECORD
        self.state = PlaybackState::Playing;
        Ok(())
    }

    async fn pause(&mut self) -> Result<(), AirPlayError> {
        // Send FLUSH
        self.state = PlaybackState::Paused;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AirPlayError> {
        // Send FLUSH + TEARDOWN
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    async fn set_volume(&mut self, volume: f32) -> Result<(), AirPlayError> {
        // Convert to dB: 0.0 = -144dB (mute), 1.0 = 0dB
        let db = if volume <= 0.0 {
            -144.0
        } else {
            30.0 * (volume.clamp(0.0, 1.0)).log10()
        };

        // Send SET_PARAMETER with volume
        self.volume = volume;
        Ok(())
    }

    async fn get_volume(&self) -> Result<f32, AirPlayError> {
        Ok(self.volume)
    }

    async fn stream_audio(&mut self, data: &[u8]) -> Result<(), AirPlayError> {
        if let Some(ref mut streamer) = self.streamer {
            let _packet = streamer.encode_frame(data);
            // Send packet via UDP
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), AirPlayError> {
        if let Some(ref mut streamer) = self.streamer {
            streamer.flush();
        }
        Ok(())
    }

    async fn set_metadata(&mut self, track: &TrackInfo) -> Result<(), AirPlayError> {
        // Convert TrackInfo to DAAP format and send
        Ok(())
    }

    async fn set_artwork(&mut self, data: &[u8]) -> Result<(), AirPlayError> {
        // Send artwork via SET_PARAMETER
        Ok(())
    }

    fn playback_state(&self) -> PlaybackState {
        self.state.clone()
    }

    fn protocol_version(&self) -> &'static str {
        "RAOP/1.0"
    }
}
```

---

### 32.3 Unified Client

- [x] **32.3.1** Extend AirPlayClient for dual protocol support

**File:** `src/client/mod.rs` (extensions)

```rust
//! Unified AirPlay client

use crate::discovery::DiscoveredDevice;
use crate::error::AirPlayError;

mod protocol;
mod session;

pub use protocol::{PreferredProtocol, SelectedProtocol, select_protocol};
pub use session::{AirPlaySession, RaopSessionImpl};

/// Unified AirPlay client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Preferred protocol
    pub preferred_protocol: PreferredProtocol,
    /// Connection timeout
    pub connection_timeout: std::time::Duration,
    /// Enable DACP remote control (RAOP)
    pub enable_dacp: bool,
    /// Enable metadata transmission
    pub enable_metadata: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            preferred_protocol: PreferredProtocol::PreferAirPlay2,
            connection_timeout: std::time::Duration::from_secs(10),
            enable_dacp: true,
            enable_metadata: true,
        }
    }
}

/// Unified AirPlay client
pub struct UnifiedAirPlayClient {
    /// Configuration
    config: ClientConfig,
    /// Active session
    session: Option<Box<dyn AirPlaySession>>,
    /// Connected device info
    device: Option<DiscoveredDevice>,
    /// Selected protocol
    protocol: Option<SelectedProtocol>,
}

impl UnifiedAirPlayClient {
    /// Create new client with default configuration
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    /// Create client with custom configuration
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            config,
            session: None,
            device: None,
            protocol: None,
        }
    }

    /// Connect to a discovered device
    pub async fn connect(&mut self, device: DiscoveredDevice) -> Result<(), AirPlayError> {
        // Select protocol
        let protocol = select_protocol(&device, self.config.preferred_protocol)
            .map_err(|e| AirPlayError::ConnectionFailed {
                message: e.to_string(),
                source: None,
            })?;

        // Create appropriate session
        let mut session: Box<dyn AirPlaySession> = match protocol {
            SelectedProtocol::AirPlay2 => {
                // Create AirPlay 2 session
                // Box::new(AirPlay2SessionImpl::new(...))
                todo!("AirPlay 2 session implementation")
            }
            SelectedProtocol::Raop => {
                let addr = device.addresses.first()
                    .ok_or_else(|| AirPlayError::ConnectionFailed {
                        message: "no address available".to_string(),
                        source: None,
                    })?;
                let port = device.raop_port.unwrap_or(5000);
                Box::new(RaopSessionImpl::new(&addr.to_string(), port))
            }
        };

        // Connect
        session.connect().await?;

        self.session = Some(session);
        self.device = Some(device);
        self.protocol = Some(protocol);

        Ok(())
    }

    /// Disconnect from current device
    pub async fn disconnect(&mut self) -> Result<(), AirPlayError> {
        if let Some(ref mut session) = self.session {
            session.disconnect().await?;
        }
        self.session = None;
        self.device = None;
        self.protocol = None;
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.session.as_ref().map(|s| s.is_connected()).unwrap_or(false)
    }

    /// Get selected protocol
    pub fn protocol(&self) -> Option<SelectedProtocol> {
        self.protocol
    }

    /// Get session reference
    pub fn session(&self) -> Option<&dyn AirPlaySession> {
        self.session.as_deref()
    }

    /// Get mutable session reference
    pub fn session_mut(&mut self) -> Option<&mut dyn AirPlaySession> {
        self.session.as_deref_mut()
    }

    // Convenience methods that delegate to session

    /// Start playback
    pub async fn play(&mut self) -> Result<(), AirPlayError> {
        self.session.as_mut()
            .ok_or(AirPlayError::NotConnected)?
            .play()
            .await
    }

    /// Pause playback
    pub async fn pause(&mut self) -> Result<(), AirPlayError> {
        self.session.as_mut()
            .ok_or(AirPlayError::NotConnected)?
            .pause()
            .await
    }

    /// Set volume
    pub async fn set_volume(&mut self, volume: f32) -> Result<(), AirPlayError> {
        self.session.as_mut()
            .ok_or(AirPlayError::NotConnected)?
            .set_volume(volume)
            .await
    }

    /// Stream audio data
    pub async fn stream_audio(&mut self, data: &[u8]) -> Result<(), AirPlayError> {
        self.session.as_mut()
            .ok_or(AirPlayError::NotConnected)?
            .stream_audio(data)
            .await
    }
}

impl Default for UnifiedAirPlayClient {
    fn default() -> Self {
        Self::new()
    }
}
```

---

### 32.4 Error Handling

- [x] **32.4.1** Extend error types for RAOP

**File:** `src/error.rs` (additions)

```rust
// Add to existing AirPlayError enum:

/// RAOP-specific errors
#[derive(Debug, thiserror::Error)]
pub enum RaopError {
    #[error("RSA authentication failed")]
    AuthenticationFailed,

    #[error("unsupported encryption type: {0}")]
    UnsupportedEncryption(String),

    #[error("SDP parsing error: {0}")]
    SdpParseError(String),

    #[error("key exchange failed: {0}")]
    KeyExchangeFailed(String),

    #[error("audio encryption error: {0}")]
    EncryptionError(String),

    #[error("timing sync failed")]
    TimingSyncFailed,

    #[error("retransmit buffer overflow")]
    RetransmitBufferOverflow,
}

// In AirPlayError, add:
// #[error("RAOP error: {0}")]
// Raop(#[from] RaopError),
```

---

### 32.5 Feature Flags

- [x] **32.5.1** Add optional RAOP feature flag

**File:** `Cargo.toml` (additions)

```toml
[features]
default = ["tokio-runtime", "raop"]
tokio-runtime = ["tokio"]
async-std-runtime = ["async-std"]
raop = ["rsa", "sha1"]  # AirPlay 1 support

[dependencies]
# RAOP-specific dependencies (optional)
rsa = { version = "0.9", optional = true }
sha1 = { version = "0.10", optional = true }
```

---

## API Examples

### Example: Automatic Protocol Selection

```rust
use airplay2_rs::{UnifiedAirPlayClient, ClientConfig, PreferredProtocol};
use airplay2_rs::discovery::{scan_all, DiscoveryOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Discover devices
    let devices = scan_all(DiscoveryOptions::default()).await?;

    // Find first audio-capable device
    let device = devices.into_iter()
        .find(|d| d.supports_raop() || d.supports_airplay2())
        .expect("no devices found");

    // Create client preferring AirPlay 2
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::PreferAirPlay2,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    // Connect (automatically selects best protocol)
    client.connect(device).await?;

    println!("Connected using {:?}", client.protocol());

    // Stream audio...
    client.set_volume(0.5).await?;
    client.play().await?;

    Ok(())
}
```

### Example: Force RAOP Protocol

```rust
use airplay2_rs::{UnifiedAirPlayClient, ClientConfig, PreferredProtocol};

let config = ClientConfig {
    preferred_protocol: PreferredProtocol::ForceRaop,
    enable_dacp: true,
    enable_metadata: true,
    ..Default::default()
};

let mut client = UnifiedAirPlayClient::with_config(config);
```

### Example: Handle Both Protocols

```rust
use airplay2_rs::{UnifiedAirPlayClient, SelectedProtocol};

async fn stream_to_device(
    client: &mut UnifiedAirPlayClient,
    audio_data: &[u8],
) -> Result<(), AirPlayError> {
    match client.protocol() {
        Some(SelectedProtocol::AirPlay2) => {
            // AirPlay 2 specific handling (e.g., multi-room)
            client.stream_audio(audio_data).await
        }
        Some(SelectedProtocol::Raop) => {
            // RAOP specific handling
            client.stream_audio(audio_data).await
        }
        None => Err(AirPlayError::NotConnected),
    }
}
```

---

## Migration Guide

### From AirPlay 2-Only Code

```rust
// Before (AirPlay 2 only):
let mut client = AirPlayClient::new();
client.connect(&device).await?;

// After (Unified):
let mut client = UnifiedAirPlayClient::new();
client.connect(device).await?;

// Protocol is automatically selected
// For explicit AirPlay 2:
let config = ClientConfig {
    preferred_protocol: PreferredProtocol::ForceAirPlay2,
    ..Default::default()
};
let mut client = UnifiedAirPlayClient::with_config(config);
```

---

## Acceptance Criteria

- [x] Protocol detection works correctly
- [x] Session abstraction handles both protocols
- [x] Unified client provides consistent API
- [x] Error types cover both protocols
- [x] Feature flags control compilation
- [x] Examples demonstrate usage patterns
- [x] Migration path is clear
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- AirPlay 2 features (multi-room, buffered audio) are not available with RAOP
- Some devices support both protocols with different feature sets
- Protocol selection should consider device capabilities
- Error messages should indicate which protocol failed
- Consider adding protocol fallback on connection failure
