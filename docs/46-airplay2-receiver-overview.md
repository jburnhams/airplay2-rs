# Section 46: AirPlay 2 Receiver Overview

## Dependencies
- **Section 01**: Project Setup (crate structure)
- **Section 02**: Core Types, Errors & Config
- **Section 34**: AirPlay 1 Receiver Overview (architectural patterns)
- **Section 06**: HomeKit Pairing & Encryption (client-side primitives)

## Overview

This section introduces the AirPlay 2 receiver implementation, which allows our library to act as an AirPlay 2 speaker/receiver that iOS, macOS, and other AirPlay 2 senders can stream audio to. The receiver complements our existing AirPlay 2 client implementation.

### AirPlay 2 vs AirPlay 1 Receiver

| Aspect | AirPlay 1 (RAOP) Receiver | AirPlay 2 Receiver |
|--------|---------------------------|-------------------|
| **Service Type** | `_raop._tcp.local` | `_airplay._tcp.local` |
| **Authentication** | RSA challenge-response | HomeKit (SRP-6a) or Password |
| **Session Format** | SDP text bodies | Binary plist bodies |
| **SETUP Phases** | Single phase | Two phases (event/timing, then audio) |
| **Audio Encryption** | AES-128-CTR | ChaCha20-Poly1305 AEAD |
| **Timing Protocol** | NTP-style (custom) | PTP (Precision Time Protocol) |
| **Buffered Audio** | Not supported | Feature bit 40 (multi-room) |
| **Control Encryption** | Plaintext RTSP | Encrypted after pairing |

### Architecture

The AirPlay 2 receiver follows the same **sans-IO** principles as the rest of the library:

```
┌─────────────────────────────────────────────────────────────────┐
│                      AirPlay2Receiver                           │
│  (High-level API - orchestrates all components)                 │
└─────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ Service         │  │ Session         │  │ Audio           │
│ Advertisement   │  │ Manager         │  │ Pipeline        │
│ (mDNS)          │  │                 │  │                 │
└─────────────────┘  └────────┬────────┘  └─────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ RTSP/HTTP       │  │ Pairing         │  │ RTP             │
│ Server          │  │ Handler         │  │ Receiver        │
│ (sans-IO)       │  │ (Server-side)   │  │ (Decryption)    │
└─────────────────┘  └─────────────────┘  └─────────────────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Reused Protocol Layer                        │
│  (Binary Plist, Crypto, RTP Packets, RTSP Codec)                │
└─────────────────────────────────────────────────────────────────┘
```

### Session Flow

A typical AirPlay 2 receiver session:

```
Sender (iOS/macOS)                    Receiver (Us)
       │                                    │
       │──── mDNS Discovery ───────────────▶│ _airplay._tcp
       │                                    │
       │──── TCP Connect ──────────────────▶│ Port 7000
       │                                    │
       │──── GET /info ────────────────────▶│
       │◀─── Device capabilities ──────────│
       │                                    │
       │──── POST /pair-setup (M1) ────────▶│
       │◀─── SRP response (M2) ────────────│
       │──── POST /pair-setup (M3) ────────▶│
       │◀─── SRP response (M4) ────────────│
       │                                    │
       │──── POST /pair-verify (M1) ───────▶│
       │◀─── Verify response (M2) ─────────│
       │──── POST /pair-verify (M3) ───────▶│
       │◀─── Verify response (M4) ─────────│
       │                                    │
       │════ Control channel now encrypted ═│
       │                                    │
       │──── SETUP (event+timing) ─────────▶│ Phase 1
       │◀─── ports allocated ──────────────│
       │                                    │
       │──── SETUP (audio streams) ────────▶│ Phase 2
       │◀─── audio ports allocated ────────│
       │                                    │
       │──── RECORD ───────────────────────▶│
       │◀─── OK + latency ─────────────────│
       │                                    │
       │════ RTP audio stream (encrypted) ══│
       │                                    │
       │──── SET_PARAMETER (volume) ───────▶│
       │──── SET_PARAMETER (metadata) ─────▶│
       │                                    │
       │──── TEARDOWN ─────────────────────▶│
       │◀─── OK ───────────────────────────│
```

## Objectives

- Define the AirPlay 2 receiver architecture
- Identify all reusable components from existing implementation
- Establish the component structure and dependencies
- Create a roadmap for implementation sections

---

## Tasks

### 46.1 Component Mapping

- [ ] **46.1.1** Map existing components to receiver needs

**Reuse Analysis:**

| Receiver Need | Existing Component | Reuse Strategy |
|--------------|-------------------|----------------|
| Service advertisement | `discovery/` module | Extend with `advertise()` |
| RTSP parsing | `protocol/rtsp/server_codec.rs` | Direct reuse |
| Binary plist | `protocol/plist/` | Direct reuse |
| SRP-6a | `protocol/pairing/setup.rs` | Adapt for server role |
| Pair-Verify | `protocol/pairing/verify.rs` | Adapt for server role |
| ChaCha20-Poly1305 | `protocol/crypto/chacha.rs` | Direct reuse |
| Ed25519 | `protocol/crypto/ed25519.rs` | Direct reuse |
| X25519 | `protocol/crypto/x25519.rs` | Direct reuse |
| HKDF | `protocol/crypto/hkdf.rs` | Direct reuse |
| RTP packets | `protocol/rtp/packet.rs` | Direct reuse |
| Jitter buffer | `audio/jitter.rs` | Adapt from AP1 receiver |
| DMAP metadata | `protocol/daap/` | Direct reuse |
| Session state | `receiver/session.rs` | Extend for AP2 states |

---

### 46.2 New Components Required

- [ ] **46.2.1** Identify components that must be created new

**New Components:**

| Component | Purpose | Section |
|-----------|---------|---------|
| `Ap2ServiceAdvertiser` | Advertise `_airplay._tcp` with feature bits | 47 |
| `PairingServer` | Server-side SRP-6a responder | 49 |
| `PasswordAuthHandler` | Password fallback authentication | 50 |
| `InfoEndpoint` | Handle GET /info requests | 51 |
| `Ap2SetupHandler` | Two-phase SETUP processing | 52 |
| `EncryptedChannel` | HAP-style encrypted framing | 53 |
| `Ap2RtpDecryptor` | ChaCha20-Poly1305 audio decryption | 54 |
| `PtpClock` | PTP timing synchronization | 55 |
| `MultiRoomCoordinator` | Buffered audio / group sync | 57 |
| `CommandHandler` | /command endpoint processing | 59 |
| `FeedbackHandler` | /feedback endpoint processing | 59 |

---

### 46.3 Directory Structure

- [ ] **46.3.1** Define the receiver module structure

**File:** `src/receiver/mod.rs` (extended)

```rust
//! AirPlay Receiver Implementation
//!
//! This module contains both AirPlay 1 (RAOP) and AirPlay 2 receiver
//! implementations. They share common infrastructure where possible.

// === Shared Infrastructure ===
pub mod session;
pub mod session_manager;
pub mod rtsp_handler;

// === AirPlay 1 (RAOP) Receiver ===
pub mod announce_handler;  // SDP parsing

// === AirPlay 2 Receiver ===
pub mod ap2;  // AirPlay 2 specific modules

// Re-exports for convenience
pub use session::{ReceiverSession, SessionState};
pub use session_manager::SessionManager;

// AirPlay 2 specific re-exports
pub use ap2::{
    AirPlay2Receiver,
    Ap2Config,
    PairingServer,
    InfoEndpoint,
};
```

**File:** `src/receiver/ap2/mod.rs`

```rust
//! AirPlay 2 Receiver Components
//!
//! This module contains AirPlay 2 specific receiver functionality.
//! It builds on shared infrastructure and reuses protocol primitives
//! from the client implementation.

pub mod config;
pub mod receiver;
pub mod pairing_server;
pub mod password_auth;
pub mod info_endpoint;
pub mod setup_handler;
pub mod encrypted_channel;
pub mod rtp_decryptor;
pub mod ptp_clock;
pub mod command_handler;
pub mod feedback_handler;
pub mod multi_room;

// Re-exports
pub use config::Ap2Config;
pub use receiver::AirPlay2Receiver;
pub use pairing_server::PairingServer;
pub use info_endpoint::InfoEndpoint;
```

---

### 46.4 Configuration Types

- [ ] **46.4.1** Define AirPlay 2 receiver configuration

**File:** `src/receiver/ap2/config.rs`

```rust
//! Configuration for AirPlay 2 Receiver

use crate::types::AudioFormat;

/// Configuration for an AirPlay 2 receiver instance
#[derive(Debug, Clone)]
pub struct Ap2Config {
    /// Device name (shown to senders)
    pub name: String,

    /// Unique device ID (typically MAC address format: AA:BB:CC:DD:EE:FF)
    pub device_id: String,

    /// Model identifier (e.g., "Receiver1,1")
    pub model: String,

    /// Manufacturer name
    pub manufacturer: String,

    /// Serial number (optional)
    pub serial_number: Option<String>,

    /// Firmware version
    pub firmware_version: String,

    /// RTSP/HTTP server port (default: 7000)
    pub server_port: u16,

    /// Enable password authentication
    pub password: Option<String>,

    /// Supported audio formats
    pub audio_formats: Vec<AudioFormat>,

    /// Enable multi-room support (feature bit 40)
    pub multi_room_enabled: bool,

    /// Audio buffer size in milliseconds
    pub buffer_size_ms: u32,

    /// Maximum concurrent sessions (usually 1)
    pub max_sessions: usize,

    /// Enable verbose protocol logging
    pub debug_logging: bool,
}

impl Default for Ap2Config {
    fn default() -> Self {
        Self {
            name: "AirPlay Receiver".to_string(),
            device_id: Self::generate_device_id(),
            model: "Receiver1,1".to_string(),
            manufacturer: "airplay2-rs".to_string(),
            serial_number: None,
            firmware_version: env!("CARGO_PKG_VERSION").to_string(),
            server_port: 7000,
            password: None,
            audio_formats: vec![
                AudioFormat::Pcm,
                AudioFormat::Alac,
                AudioFormat::AacEld,
            ],
            multi_room_enabled: true,
            buffer_size_ms: 2000,
            max_sessions: 1,
            debug_logging: false,
        }
    }
}

impl Ap2Config {
    /// Create a new configuration with the given device name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set password protection
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Disable multi-room support
    pub fn without_multi_room(mut self) -> Self {
        self.multi_room_enabled = false;
        self
    }

    /// Set custom server port
    pub fn with_port(mut self, port: u16) -> Self {
        self.server_port = port;
        self
    }

    /// Generate a random device ID in MAC address format
    fn generate_device_id() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 6] = rng.gen();
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
        )
    }

    /// Calculate feature flags based on configuration
    pub fn feature_flags(&self) -> u64 {
        let mut flags: u64 = 0;

        // Core features (always enabled)
        flags |= 1 << 0;   // Video supported (even if we only do audio)
        flags |= 1 << 1;   // Photo supported
        flags |= 1 << 7;   // Audio
        flags |= 1 << 9;   // Audio redundant (FEC)
        flags |= 1 << 14;  // MFi soft auth
        flags |= 1 << 17;  // Supports pairing
        flags |= 1 << 18;  // Supports PIN pairing
        flags |= 1 << 27;  // Supports unified media control

        // Optional features
        if self.multi_room_enabled {
            flags |= 1 << 40;  // Buffered audio
            flags |= 1 << 41;  // PTP clock
            flags |= 1 << 46;  // HomeKit pairing
        }

        if self.password.is_some() {
            flags |= 1 << 15;  // Password required
        }

        flags
    }

    /// Get status flags for TXT record
    pub fn status_flags(&self) -> u32 {
        let mut flags: u32 = 0;

        // Bit 2: Problem detected (0 = no problem)
        // Bit 3: Supports PIN (1 = yes)
        flags |= 1 << 3;

        // Bit 4: Supports password
        if self.password.is_some() {
            flags |= 1 << 4;
        }

        flags
    }
}

/// Builder for Ap2Config with validation
pub struct Ap2ConfigBuilder {
    config: Ap2Config,
}

impl Ap2ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Ap2Config::default(),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.config.device_id = id.into();
        self
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.config.password = Some(password.into());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    pub fn buffer_size_ms(mut self, ms: u32) -> Self {
        self.config.buffer_size_ms = ms;
        self
    }

    pub fn build(self) -> Result<Ap2Config, ConfigError> {
        // Validate configuration
        if self.config.name.is_empty() {
            return Err(ConfigError::InvalidName("Name cannot be empty".into()));
        }

        if self.config.device_id.len() != 17 {
            return Err(ConfigError::InvalidDeviceId(
                "Device ID must be in MAC address format".into()
            ));
        }

        if self.config.server_port == 0 {
            return Err(ConfigError::InvalidPort("Port cannot be 0".into()));
        }

        Ok(self.config)
    }
}

impl Default for Ap2ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid device name: {0}")]
    InvalidName(String),

    #[error("Invalid device ID: {0}")]
    InvalidDeviceId(String),

    #[error("Invalid port: {0}")]
    InvalidPort(String),
}
```

---

### 46.5 Session States

- [ ] **46.5.1** Define AirPlay 2 receiver session states

**File:** `src/receiver/ap2/session_state.rs`

```rust
//! AirPlay 2 Receiver Session State Machine
//!
//! AirPlay 2 sessions have more states than AirPlay 1 due to
//! multi-phase setup and encrypted control channels.

/// Session state for AirPlay 2 receiver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ap2SessionState {
    /// Initial state - TCP connected, awaiting requests
    Connected,

    /// /info requested - sender is querying capabilities
    InfoExchanged,

    /// Pair-setup in progress (SRP exchange)
    PairingSetup {
        /// Current step in SRP protocol (1-4)
        step: u8,
    },

    /// Pair-verify in progress
    PairingVerify {
        /// Current step in verify protocol (1-4)
        step: u8,
    },

    /// Pairing complete - control channel now encrypted
    Paired,

    /// First SETUP complete (event + timing channels)
    SetupPhase1,

    /// Second SETUP complete (audio channels)
    SetupPhase2,

    /// RECORD received - streaming active
    Streaming,

    /// Paused (audio stopped but session alive)
    Paused,

    /// Session ending
    Teardown,

    /// Error state
    Error {
        code: u32,
        message: String,
    },
}

impl Ap2SessionState {
    /// Check if this state allows the given RTSP method
    pub fn allows_method(&self, method: &str) -> bool {
        match self {
            Self::Connected => matches!(method, "OPTIONS" | "GET" | "POST"),
            Self::InfoExchanged => matches!(method, "OPTIONS" | "GET" | "POST"),
            Self::PairingSetup { .. } => matches!(method, "OPTIONS" | "POST"),
            Self::PairingVerify { .. } => matches!(method, "OPTIONS" | "POST"),
            Self::Paired => matches!(
                method,
                "OPTIONS" | "GET" | "POST" | "SETUP" | "GET_PARAMETER" | "SET_PARAMETER"
            ),
            Self::SetupPhase1 => matches!(
                method,
                "OPTIONS" | "SETUP" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::SetupPhase2 => matches!(
                method,
                "OPTIONS" | "RECORD" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::Streaming => matches!(
                method,
                "OPTIONS" | "GET_PARAMETER" | "SET_PARAMETER" | "FLUSH" | "TEARDOWN" | "POST"
            ),
            Self::Paused => matches!(
                method,
                "OPTIONS" | "RECORD" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::Teardown => matches!(method, "OPTIONS"),
            Self::Error { .. } => false,
        }
    }

    /// Check if the session is in an authenticated state
    pub fn is_authenticated(&self) -> bool {
        matches!(
            self,
            Self::Paired
                | Self::SetupPhase1
                | Self::SetupPhase2
                | Self::Streaming
                | Self::Paused
        )
    }

    /// Check if the session is actively streaming
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::Streaming)
    }

    /// Check if the control channel should be encrypted
    pub fn requires_encryption(&self) -> bool {
        // After pairing completes, all control traffic is encrypted
        self.is_authenticated()
    }
}

/// State transition validation
impl Ap2SessionState {
    /// Attempt to transition to a new state
    pub fn transition_to(&self, new_state: Ap2SessionState) -> Result<Ap2SessionState, StateError> {
        let valid = match (self, &new_state) {
            // From Connected
            (Self::Connected, Self::InfoExchanged) => true,
            (Self::Connected, Self::PairingSetup { step: 1 }) => true,

            // From InfoExchanged
            (Self::InfoExchanged, Self::PairingSetup { step: 1 }) => true,

            // Pairing setup progression
            (Self::PairingSetup { step: 1 }, Self::PairingSetup { step: 2 }) => true,
            (Self::PairingSetup { step: 2 }, Self::PairingSetup { step: 3 }) => true,
            (Self::PairingSetup { step: 3 }, Self::PairingSetup { step: 4 }) => true,
            (Self::PairingSetup { step: 4 }, Self::PairingVerify { step: 1 }) => true,

            // Pairing verify progression
            (Self::PairingVerify { step: 1 }, Self::PairingVerify { step: 2 }) => true,
            (Self::PairingVerify { step: 2 }, Self::PairingVerify { step: 3 }) => true,
            (Self::PairingVerify { step: 3 }, Self::PairingVerify { step: 4 }) => true,
            (Self::PairingVerify { step: 4 }, Self::Paired) => true,

            // From Paired
            (Self::Paired, Self::SetupPhase1) => true,

            // From SetupPhase1
            (Self::SetupPhase1, Self::SetupPhase2) => true,
            (Self::SetupPhase1, Self::Teardown) => true,

            // From SetupPhase2
            (Self::SetupPhase2, Self::Streaming) => true,
            (Self::SetupPhase2, Self::Teardown) => true,

            // From Streaming
            (Self::Streaming, Self::Paused) => true,
            (Self::Streaming, Self::Teardown) => true,

            // From Paused
            (Self::Paused, Self::Streaming) => true,
            (Self::Paused, Self::Teardown) => true,

            // Error can be reached from anywhere
            (_, Self::Error { .. }) => true,

            // Teardown can be reached from most states
            (_, Self::Teardown) if !matches!(self, Self::Connected | Self::Error { .. }) => true,

            _ => false,
        };

        if valid {
            Ok(new_state)
        } else {
            Err(StateError::InvalidTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", new_state),
            })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pairing_flow() {
        let mut state = Ap2SessionState::Connected;

        state = state.transition_to(Ap2SessionState::InfoExchanged).unwrap();
        state = state.transition_to(Ap2SessionState::PairingSetup { step: 1 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingSetup { step: 2 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingSetup { step: 3 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingSetup { step: 4 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingVerify { step: 1 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingVerify { step: 2 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingVerify { step: 3 }).unwrap();
        state = state.transition_to(Ap2SessionState::PairingVerify { step: 4 }).unwrap();
        state = state.transition_to(Ap2SessionState::Paired).unwrap();

        assert!(state.is_authenticated());
        assert!(state.requires_encryption());
    }

    #[test]
    fn test_invalid_transition() {
        let state = Ap2SessionState::Connected;

        // Cannot go directly to Streaming
        let result = state.transition_to(Ap2SessionState::Streaming);
        assert!(result.is_err());
    }

    #[test]
    fn test_method_permissions() {
        let state = Ap2SessionState::Connected;
        assert!(state.allows_method("OPTIONS"));
        assert!(state.allows_method("GET"));
        assert!(!state.allows_method("SETUP"));

        let state = Ap2SessionState::Paired;
        assert!(state.allows_method("SETUP"));
        assert!(!state.allows_method("RECORD"));

        let state = Ap2SessionState::SetupPhase2;
        assert!(state.allows_method("RECORD"));
    }
}
```

---

## Acceptance Criteria

- [ ] Component mapping complete with reuse strategy for each
- [ ] New component list identifies all required implementations
- [ ] Directory structure defined and documented
- [ ] Configuration types support all receiver options
- [ ] Session state machine handles full AirPlay 2 flow
- [ ] State transitions validated with tests
- [ ] Feature flags correctly calculated from config

---

## Notes

### Maximum Code Reuse Strategy

The implementation prioritizes reusing existing code:

1. **Direct Reuse** - Components used as-is:
   - Binary plist codec
   - All cryptographic primitives
   - RTP packet structures
   - RTSP server codec
   - DMAP/DAAP metadata parsing

2. **Adaptation** - Components modified for receiver role:
   - Pairing (client → server role)
   - Session management (extended states)
   - Jitter buffer (AP1 → AP2 timing)

3. **New Implementation** - Components without client equivalent:
   - PTP clock synchronization
   - Multi-room coordination
   - /info, /command, /feedback endpoints

### Testing Strategy

Testing is divided into:

1. **Unit Tests** - Individual component testing with mocks
2. **Integration Tests** - Full session flows with mock senders
3. **Capture Verification** - Real iOS/macOS packet captures replayed

Real device testing is out of scope for initial implementation.

---

## References

- [Unofficial AirPlay 2 Protocol](https://emanuelecozzi.net/docs/airplay2)
- [AirPlay 2 Receiver Analysis](https://github.com/openairplay/airplay2-receiver)
- [shairport-sync](https://github.com/mikebrady/shairport-sync) (AirPlay 1 reference)
- [HomeKit Accessory Protocol Specification](https://developer.apple.com/homekit/)
