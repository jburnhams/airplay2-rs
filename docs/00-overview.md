# AirPlay Rust Library - Developer Overview

## Introduction

This document provides an overview for developers working on the `airplay2-rs` library, a pure Rust implementation for streaming audio to AirPlay compatible devices. The library supports both **AirPlay 2** (modern protocol) and **AirPlay 1/RAOP** (legacy protocol) for maximum device compatibility.

## Project Goals

- **Pure Rust**: No C dependencies, cross-platform (Linux, macOS, Windows)
- **Sans-IO Architecture**: Protocol logic separated from I/O for testability
- **Modern Rust**: Current stable toolchain, idiomatic patterns
- **Quality First**: Comprehensive testing, documentation, and best practices
- **Extensible**: Designed for future FairPlay support without major refactoring
- **Protocol Flexibility**: Unified API supporting both AirPlay 1 and AirPlay 2 protocols

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Public API Layer                            │
│  ┌─────────────────┐  ┌──────────────────┐  ┌───────────────────┐  │
│  │  AirPlayPlayer  │  │  AirPlayClient   │  │   AirPlayGroup    │  │
│  │  (high-level)   │  │  (mid-level)     │  │   (multi-room)    │  │
│  └────────┬────────┘  └────────┬─────────┘  └─────────┬─────────┘  │
└───────────┼────────────────────┼──────────────────────┼────────────┘
            │                    │                      │
┌───────────┴────────────────────┴──────────────────────┴────────────┐
│                        Control Layer                                │
│  ┌──────────────┐  ┌───────────────┐  ┌────────────┐  ┌─────────┐  │
│  │   Playback   │  │     Queue     │  │   State    │  │ Volume  │  │
│  │   Control    │  │  Management   │  │  Events    │  │ Control │  │
│  └──────┬───────┘  └───────┬───────┘  └─────┬──────┘  └────┬────┘  │
└─────────┼──────────────────┼────────────────┼──────────────┼───────┘
          │                  │                │              │
┌─────────┴──────────────────┴────────────────┴──────────────┴───────┐
│                         Audio Layer                                 │
│  ┌────────────────┐  ┌─────────────────┐  ┌──────────────────────┐ │
│  │  PCM Streaming │  │  URL Streaming  │  │  Buffering & Timing  │ │
│  └───────┬────────┘  └────────┬────────┘  └───────────┬──────────┘ │
└──────────┼────────────────────┼──────────────────────┼─────────────┘
           │                    │                      │
┌──────────┴────────────────────┴──────────────────────┴─────────────┐
│                      Connection Layer                               │
│  ┌────────────────────┐  ┌──────────────────────────────────────┐  │
│  │ Connection Manager │  │  Async Runtime Abstraction           │  │
│  └─────────┬──────────┘  └──────────────────┬───────────────────┘  │
└────────────┼────────────────────────────────┼──────────────────────┘
             │                                │
┌────────────┴────────────────────────────────┴──────────────────────┐
│                   Protocol Layer (Sans-IO)                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │   RTSP   │  │   RTP    │  │   HomeKit    │  │  Binary Plist  │  │
│  │ Protocol │  │ Protocol │  │   Pairing    │  │     Codec      │  │
│  └──────────┘  └──────────┘  └──────────────┘  └────────────────┘  │
└────────────────────────────────────────────────────────────────────┘
             │
┌────────────┴───────────────────────────────────────────────────────┐
│                      Foundation Layer                               │
│  ┌─────────────┐  ┌───────────────┐  ┌──────────────────────────┐  │
│  │ Core Types  │  │    Crypto     │  │   mDNS Discovery         │  │
│  │  & Errors   │  │  Primitives   │  │                          │  │
│  └─────────────┘  └───────────────┘  └──────────────────────────┘  │
└────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
airplay2-rs/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Public API exports
│   │
│   ├── types/                    # Core types
│   │   ├── mod.rs
│   │   ├── device.rs             # AirPlayDevice
│   │   ├── track.rs              # TrackInfo
│   │   ├── state.rs              # PlaybackState, RepeatMode
│   │   └── config.rs             # AirPlayConfig
│   │
│   ├── error.rs                  # AirPlayError enum
│   │
│   ├── protocol/                 # Sans-IO protocol implementations
│   │   ├── mod.rs
│   │   ├── plist/                # Binary plist codec
│   │   │   ├── mod.rs
│   │   │   ├── encode.rs
│   │   │   └── decode.rs
│   │   ├── crypto/               # Cryptographic primitives
│   │   │   ├── mod.rs
│   │   │   ├── srp.rs            # SRP-6a
│   │   │   ├── ed25519.rs        # Ed25519 signatures
│   │   │   ├── x25519.rs         # X25519 key exchange
│   │   │   ├── hkdf.rs           # HKDF key derivation
│   │   │   ├── chacha.rs         # ChaCha20-Poly1305
│   │   │   └── aes.rs            # AES-CTR, AES-GCM
│   │   ├── rtsp/                 # RTSP protocol
│   │   │   ├── mod.rs
│   │   │   ├── request.rs
│   │   │   ├── response.rs
│   │   │   ├── codec.rs          # Sans-IO encode/decode
│   │   │   └── session.rs        # Session state machine
│   │   ├── rtp/                  # RTP/RAOP protocol
│   │   │   ├── mod.rs
│   │   │   ├── packet.rs
│   │   │   ├── timing.rs
│   │   │   └── codec.rs
│   │   └── pairing/              # HomeKit pairing
│   │       ├── mod.rs
│   │       ├── transient.rs
│   │       ├── persistent.rs
│   │       └── storage.rs
│   │
│   ├── discovery/                # mDNS device discovery
│   │   ├── mod.rs
│   │   ├── browser.rs
│   │   └── parser.rs             # TXT record parsing
│   │
│   ├── net/                      # Network abstraction
│   │   ├── mod.rs
│   │   ├── traits.rs             # AsyncRead/Write traits
│   │   ├── tokio.rs              # Tokio implementation
│   │   └── async_std.rs          # async-std implementation (feature)
│   │
│   ├── connection/               # Connection management
│   │   ├── mod.rs
│   │   ├── manager.rs
│   │   └── state.rs
│   │
│   ├── audio/                    # Audio handling
│   │   ├── mod.rs
│   │   ├── format.rs             # Audio format detection
│   │   ├── buffer.rs             # Buffering
│   │   ├── timing.rs             # Synchronization
│   │   ├── pcm.rs                # PCM streaming
│   │   └── url.rs                # URL-based streaming
│   │
│   ├── control/                  # Playback control
│   │   ├── mod.rs
│   │   ├── playback.rs
│   │   ├── queue.rs
│   │   ├── volume.rs
│   │   └── events.rs
│   │
│   ├── group/                    # Multi-room support
│   │   ├── mod.rs
│   │   ├── coordinator.rs
│   │   └── sync.rs
│   │
│   ├── client.rs                 # AirPlayClient
│   └── player.rs                 # AirPlayPlayer (high-level)
│
├── tests/
│   ├── common/
│   │   └── mod.rs                # Test utilities
│   ├── mock_server/              # Mock AirPlay device
│   │   ├── mod.rs
│   │   ├── server.rs
│   │   ├── handlers.rs
│   │   └── state.rs
│   ├── integration/
│   │   ├── discovery_tests.rs
│   │   ├── connection_tests.rs
│   │   ├── playback_tests.rs
│   │   └── multiroom_tests.rs
│   └── protocol/
│       ├── rtsp_tests.rs
│       ├── rtp_tests.rs
│       └── pairing_tests.rs
│
├── examples/
│   ├── discover.rs               # Device discovery
│   ├── play_url.rs               # URL playback
│   ├── play_pcm.rs               # PCM streaming
│   └── multi_room.rs             # Multi-room example
│
└── docs/
    ├── 00-overview.md            # This document
    ├── 01-23-*.md                # AirPlay 2 section documents
    └── 24-33-*.md                # AirPlay 1 (RAOP) section documents
```

## Section Dependencies

The following diagram shows dependencies between implementation sections:

```
                    ┌──────────────────┐
                    │  01: Project     │
                    │  Setup & CI/CD   │
                    └────────┬─────────┘
                             │
                    ┌────────▼─────────┐
                    │  02: Core Types  │
                    │  Errors & Config │
                    └────────┬─────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
┌───────▼───────┐   ┌────────▼────────┐   ┌──────▼───────┐
│ 03: Binary    │   │ 04: Crypto      │   │ 08: mDNS     │
│ Plist Codec   │   │ Primitives      │   │ Discovery    │
└───────┬───────┘   └────────┬────────┘   └──────────────┘
        │                    │
        │           ┌────────┴────────┐
        │           │                 │
┌───────┴───────────▼───┐   ┌─────────▼─────────┐
│ 05: RTSP Protocol     │   │ 07: HomeKit       │
│ (Sans-IO)             │   │ Pairing Protocol  │
└───────────┬───────────┘   └─────────┬─────────┘
            │                         │
┌───────────▼───────────┐             │
│ 06: RTP/RAOP Protocol │             │
│ (Sans-IO)             │             │
└───────────┬───────────┘             │
            │                         │
            │    ┌────────────────────┘
            │    │
    ┌───────▼────▼──────┐
    │ 09: Async Runtime │
    │ Abstraction       │
    └────────┬──────────┘
             │
    ┌────────▼──────────┐       ┌────────────────────┐
    │ 10: Connection    │◄──────│ 20: Mock AirPlay   │
    │ Management        │       │ Server             │
    └────────┬──────────┘       └────────────────────┘
             │
    ┌────────┴──────────────────┬──────────────────┐
    │                           │                  │
┌───▼────────────┐    ┌─────────▼──────┐    ┌──────▼─────┐
│ 11: Audio      │    │ 12: Audio      │    │ 18: Volume │
│ Format/Codec   │    │ Buffer/Timing  │    │ Control    │
└───────┬────────┘    └────────┬───────┘    └────────────┘
        │                      │
   ┌────┴──────┬───────────────┤
   │           │               │
┌──▼───┐    ┌──▼────┐    ┌─────▼─────────┐
│ 13:  │    │ 14:   │    │ 15: Playback  │
│ PCM  │    │ URL   │    │ Control       │
└──┬───┘    └──┬────┘    └───────┬───────┘
   │           │                 │
   │           │         ┌───────┴───────┐
   │           │         │               │
   │           │    ┌────▼────┐    ┌─────▼─────┐
   │           │    │ 16:     │    │ 17: State │
   │           │    │ Queue   │    │ & Events  │
   │           │    └────┬────┘    └─────┬─────┘
   │           │         │               │
   └───────────┴─────────┴───────┬───────┘
                                 │
                    ┌────────────▼────────────┐
                    │ 19: Multi-room Grouping │
                    └────────────┬────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ 21: AirPlayClient       │
                    │ Implementation          │
                    └────────────┬────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ 22: AirPlayPlayer       │
                    │ High-level Wrapper      │
                    └─────────────────────────┘
```

## AirPlay 1 (RAOP) Support

The library includes comprehensive documentation for AirPlay 1 (RAOP - Remote Audio Output Protocol) support, enabling compatibility with older AirPlay receivers. The implementation leverages shared components with AirPlay 2 where possible.

### AirPlay 1 Section Dependencies

```
                    ┌──────────────────┐
                    │ 24: AirPlay 1    │
                    │ Overview         │
                    └────────┬─────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
┌───────▼───────┐   ┌────────▼────────┐   ┌──────▼───────┐
│ 25: RAOP      │   │ 26: RSA         │   │ 29: RAOP     │
│ Discovery     │   │ Authentication  │   │ Encryption   │
└───────┬───────┘   └────────┬────────┘   └──────┬───────┘
        │                    │                   │
        │           ┌────────┴────────┐          │
        │           │                 │          │
        └───────────▼─────────────────▼──────────┘
                    │
           ┌────────▼────────┐
           │ 27: RTSP/RAOP   │
           │ Session Mgmt    │
           └────────┬────────┘
                    │
           ┌────────▼────────┐
           │ 28: RTP/RAOP    │
           │ Streaming       │
           └────────┬────────┘
                    │
        ┌───────────┼───────────┐
        │           │           │
┌───────▼───────┐   │   ┌───────▼───────┐
│ 30: DACP      │   │   │ 31: DAAP      │
│ Remote Ctrl   │   │   │ Metadata      │
└───────────────┘   │   └───────────────┘
                    │
           ┌────────▼────────┐
           │ 32: AirPlay 1   │
           │ Integration     │
           └────────┬────────┘
                    │
           ┌────────▼────────┐
           │ 33: AirPlay 1   │
           │ Testing         │
           └─────────────────┘
```

### AirPlay 1 Documentation Sections

| Section | Title | Description |
|---------|-------|-------------|
| 24 | AirPlay 1 Overview | Architecture comparison, unified client design |
| 25 | RAOP Discovery | mDNS `_raop._tcp` service discovery and TXT record parsing |
| 26 | RSA Authentication | RSA-OAEP key exchange and Apple-Challenge response |
| 27 | RTSP/RAOP Session | SDP-based session establishment and RTSP extensions |
| 28 | RTP/RAOP Streaming | Audio streaming with timing sync and retransmission |
| 29 | RAOP Encryption | AES-128-CTR encryption for audio payloads |
| 30 | DACP Remote Control | Digital Audio Control Protocol for playback commands |
| 31 | DAAP Metadata | DMAP encoding for track info and artwork |
| 32 | AirPlay 1 Integration | Unified API design with AirPlay 2 |
| 33 | AirPlay 1 Testing | Mock server, test suites, and CI/CD |

### Shared Components

The following components are shared between AirPlay 1 and AirPlay 2:

| Component | AirPlay 1 Usage | AirPlay 2 Usage |
|-----------|-----------------|-----------------|
| RTSP Codec | SDP bodies, RAOP headers | Binary plist bodies |
| RTP Streaming | Timing/sync packets | Similar with extensions |
| mDNS Discovery | `_raop._tcp` service | `_airplay._tcp` service |
| Audio Codecs | AAC, ALAC, PCM | Same codecs |
| AES Encryption | AES-128-CTR | AES-GCM, ChaCha20-Poly1305 |

## Parallel Work Streams

Based on dependencies, here are recommended parallel work streams:

### Stream A: Foundation & Protocols
1. Section 01 → 02 → 03 → 05 → 06

### Stream B: Crypto & Pairing
1. Section 01 → 02 → 04 → 07

### Stream C: Discovery
1. Section 01 → 02 → 08

### Stream D: Testing Infrastructure
1. Section 01 → 02 → 20 (can start mock server early)

### Convergence Point
After Streams A, B, C complete → Section 09, 10 → remaining sections

### Stream E: AirPlay 1 Support
1. Section 24 (Overview) → 25, 26, 29 (parallel) → 27 → 28 → 30, 31 (parallel) → 32 → 33

Note: Stream E can proceed in parallel with Streams A-D as it shares foundation components but has distinct protocol implementations.

## Key Design Principles

### 1. Sans-IO Protocol Design

All protocol implementations (RTSP, RTP, HomeKit pairing) follow the sans-IO pattern:

```rust
// Protocol logic is pure, no I/O
pub struct RtspCodec {
    // internal state (buffer, parser state)
}

impl RtspCodec {
    /// Encode a request into bytes (no I/O)
    pub fn encode_request(&self, request: &RtspRequest) -> Vec<u8>;

    /// Feed received bytes into internal buffer (no I/O)
    pub fn feed(&mut self, bytes: &[u8]);

    /// Try to decode a complete response from internal buffer (no I/O)
    pub fn decode(&mut self) -> Result<Option<RtspResponse>, DecodeError>;
}
```

Benefits:
- Fully testable without network
- Runtime agnostic
- Easy to reason about state

### 2. Thin Async Wrappers

Async wrappers are minimal, delegating to sans-IO core:

```rust
pub struct RtspConnection<T: AsyncRead + AsyncWrite> {
    transport: T,
    codec: RtspCodec,
}

impl<T: AsyncRead + AsyncWrite + Unpin> RtspConnection<T> {
    pub async fn send_request(&mut self, request: RtspRequest) -> Result<RtspResponse, Error> {
        let bytes = self.codec.encode_request(&request);
        self.transport.write_all(&bytes).await?;

        loop {
            // Check if we already have a response buffered
            if let Some(response) = self.codec.decode()? {
                return Ok(response);
            }

            let mut buf = [0u8; 4096];
            let n = self.transport.read(&mut buf).await?;

            if n == 0 {
                return Err(Error::ConnectionClosed);
            }

            self.codec.feed(&buf[..n]);
        }
    }
}
```

### 3. Error Handling

Use `thiserror` for error types with clear context:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AirPlayError {
    #[error("connection failed: {message}")]
    ConnectionFailed {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    // ...
}
```

### 4. Logging with Tracing

Use `tracing` crate with structured logging:

```rust
use tracing::{debug, info, warn, error, instrument};

#[instrument(skip(self), fields(device_id = %device.id))]
pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
    debug!("initiating connection");
    // ...
}
```

### 5. Feature Flags

Optional features for additional dependencies:

```toml
[features]
default = ["tokio-runtime"]
tokio-runtime = ["tokio"]
async-std-runtime = ["async-std"]
persistent-pairing = ["sled"]  # Or similar for key storage
```

## Testing Strategy

### Unit Tests
- Located alongside source in `#[cfg(test)]` modules
- Test sans-IO protocol logic with crafted byte sequences
- Test state machines with all transition paths
- Use property-based testing where appropriate

### Integration Tests
- Located in `tests/` directory
- Use mock AirPlay server for reproducible tests
- Test full async flows end-to-end
- Cover error scenarios and edge cases

### Manual Testing
- Real hardware testing done separately
- Not part of CI pipeline
- Document manual test procedures

## Recommended Crates

| Purpose | Crate | Notes |
|---------|-------|-------|
| Async runtime | `tokio` | Default, well-maintained |
| Async traits | `async-trait` | Until RPITIT stabilizes |
| Error handling | `thiserror` | Derive Error impls |
| Logging | `tracing` | Structured, async-aware |
| mDNS | `mdns-sd` | Pure Rust, maintained |
| Crypto (SRP) | `srp` | SRP-6a implementation |
| Crypto (curves) | `x25519-dalek`, `ed25519-dalek` | Audited, pure Rust |
| Crypto (symmetric) | `chacha20poly1305`, `aes-gcm` | RustCrypto, audited |
| Crypto (HKDF) | `hkdf` | RustCrypto |
| Binary plist | Custom or `plist` | May need custom for AirPlay specifics |
| Byte handling | `bytes` | Zero-copy buffers |
| Testing | `tokio-test`, `proptest` | Async testing, property testing |

## Getting Started

1. Read Section 01 (Project Setup) first
2. Identify which work stream you're on
3. Complete sections in dependency order
4. Ensure all tests pass before marking complete
5. Update checkboxes in section documents as you go

## Communication

- All section documents include checkboxes for progress tracking
- Dependencies must be completed before dependent sections start
- Integration points are clearly marked in each section
- If blocked on a dependency, document what's needed

## References

### AirPlay 2
- [AirPlay 2 Internals](https://emanuelecozzi.net/docs/airplay2)
- [openairplay/airplay2-receiver](https://github.com/openairplay/airplay2-receiver)
- [mikebrady/shairport-sync](https://github.com/mikebrady/shairport-sync)
- [pyatv](https://pyatv.dev/)

### AirPlay 1 / RAOP
- [OpenAirPlay Specification](https://openairplay.github.io/airplay-spec/audio/index.html)
- [Unofficial AirPlay Protocol Specification](https://nto.github.io/AirPlay.html)
- [RAOP Protocol Analysis](https://git.zx2c4.com/Airtunes2/about/)
