# Section 34: AirPlay 1 Receiver Overview

## Dependencies
- **Section 02**: Core Types, Errors & Config (must be complete)
- **Section 24**: AirPlay 1 Overview (recommended reading)
- **Section 05**: RTSP Protocol (foundation for server implementation)
- **Section 06**: RTP Protocol (foundation for receiver implementation)

## Overview

This section introduces the AirPlay 1 (RAOP) **receiver** implementation, enabling `airplay2-rs` to accept incoming audio streams from AirPlay senders (iTunes, iOS devices, macOS, third-party clients). The receiver complements the existing client implementation, sharing substantial infrastructure while inverting the data flow.

### What is an AirPlay 1 Receiver?

An AirPlay 1 receiver (also known as an AirTunes or RAOP receiver):
- Advertises itself on the local network via mDNS/Bonjour
- Accepts incoming RTSP connections from senders
- Receives RTP audio packets over UDP
- Decrypts, decodes, and plays audio through local hardware
- Synchronizes playback timing with the sender

### Design Philosophy

The receiver implementation follows the same principles as the rest of the library:

1. **Sans-IO Core**: Protocol logic separated from I/O operations
2. **Trait-Based Abstractions**: Audio output via traits for platform independence
3. **Feature Flags**: Optional compilation for lightweight client-only builds
4. **Extensive Testing**: Mock senders, protocol conformance, network simulation
5. **Code Reuse**: Maximum leverage of existing protocol implementations

## Objectives

- Implement a fully functional AirPlay 1 audio receiver
- Support PCM, ALAC, and AAC audio codecs
- Support RSA+AES-128 encryption (standard RAOP security)
- Provide platform-independent audio output via traits
- Enable optional password protection (deferred, but designed for)
- Maintain compatibility with iTunes, iOS, macOS, and third-party senders
- Integrate seamlessly with existing library architecture

---

## Architecture

### Receiver vs Client: Inverted Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CLIENT (Sender) Mode                             │
│                                                                          │
│   ┌──────────────┐    RTSP Request    ┌──────────────┐                  │
│   │   AirPlay    │ ──────────────────▶│   Remote     │                  │
│   │   Client     │    RTP Audio       │   Receiver   │                  │
│   │   (us)       │ ══════════════════▶│   (device)   │                  │
│   └──────────────┘                    └──────────────┘                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                        RECEIVER (Server) Mode                            │
│                                                                          │
│   ┌──────────────┐    RTSP Request    ┌──────────────┐                  │
│   │   Remote     │ ──────────────────▶│   AirPlay    │                  │
│   │   Sender     │    RTP Audio       │   Receiver   │                  │
│   │   (iTunes)   │ ══════════════════▶│   (us)       │                  │
│   └──────────────┘                    └──────────────┘                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Receiver Architecture Layers

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Public API Layer                                │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                      AirPlayReceiver                             │   │
│   │   - start() / stop()                                            │   │
│   │   - on_audio(), on_metadata()                                   │   │
│   │   - volume control, status queries                              │   │
│   └─────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────┬────────────────────────────────┘
                                         │
┌────────────────────────────────────────▼────────────────────────────────┐
│                         Session Layer                                    │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────────────┐  │
│   │    Session     │   │    Volume &    │   │   Metadata Handler     │  │
│   │    Manager     │   │    Control     │   │   (DMAP/artwork)       │  │
│   └───────┬────────┘   └────────────────┘   └────────────────────────┘  │
└───────────┼─────────────────────────────────────────────────────────────┘
            │
┌───────────▼─────────────────────────────────────────────────────────────┐
│                          Audio Layer                                     │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────────────┐  │
│   │  Jitter Buffer │   │   Decryption   │   │    Audio Decoder       │  │
│   │  & Reordering  │   │   (AES-128)    │   │   (ALAC/AAC/PCM)       │  │
│   └───────┬────────┘   └────────────────┘   └───────────┬────────────┘  │
│           │                                             │                │
│   ┌───────▼─────────────────────────────────────────────▼────────────┐  │
│   │                      Audio Output Trait                          │  │
│   │   - CoreAudio (macOS) / ALSA (Linux) / WASAPI (Windows)         │  │
│   └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
            │
┌───────────▼─────────────────────────────────────────────────────────────┐
│                        Protocol Layer (Sans-IO)                          │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────────────┐  │
│   │  RTSP Server   │   │  RTP Receiver  │   │   Timing Sync          │  │
│   │  (sans-IO)     │   │  (sans-IO)     │   │   (NTP-like)           │  │
│   └────────────────┘   └────────────────┘   └────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
            │
┌───────────▼─────────────────────────────────────────────────────────────┐
│                        Network Layer                                     │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────────────┐  │
│   │  TCP Listener  │   │  UDP Sockets   │   │   mDNS Advertiser      │  │
│   │  (RTSP)        │   │  (Audio/Ctrl)  │   │   (_raop._tcp)         │  │
│   └────────────────┘   └────────────────┘   └────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Code Reuse Strategy

### Components Reused Directly

| Component | Location | Usage in Receiver |
|-----------|----------|-------------------|
| `RtspCodec` | `protocol/rtsp/codec.rs` | Parse requests, encode responses |
| `RtspRequest`, `RtspResponse` | `protocol/rtsp/` | Same structures, reversed roles |
| `RtpPacket`, `RtpHeader` | `protocol/rtp/packet.rs` | Receive instead of send |
| `TimingPacket` | `protocol/rtp/timing.rs` | Respond to timing requests |
| `RaopEncryption` | `protocol/raop/encryption.rs` | Decrypt incoming audio |
| `AudioRingBuffer` | `audio/buffer.rs` | Buffer decoded audio for output |
| `DmapParser` | `protocol/dacp/` | Parse incoming metadata |

### Components Extended

| Component | Extension Needed |
|-----------|-----------------|
| `discovery/` | Add service advertisement (currently browse-only) |
| `protocol/rtsp/` | Add `RtspServerCodec` for server-side parsing |
| `protocol/sdp/` | Add SDP parser (currently only encoder) |
| `audio/` | Add decoder wrappers (currently encoder-focused) |

### New Components

| Component | Purpose |
|-----------|---------|
| `receiver/` | New module for receiver-specific logic |
| `audio/output.rs` | Audio output trait and implementations |
| `receiver/session.rs` | Server-side session management |
| `receiver/timing.rs` | NTP-like timing response logic |

---

## Feature Flags

The receiver functionality is gated behind feature flags to keep client-only builds lightweight.

### Cargo.toml Additions

```toml
[features]
default = ["tokio-runtime", "client"]

# Core functionality
tokio-runtime = ["tokio", "tokio-util"]

# Client (sender) functionality - existing code
client = []

# Receiver functionality - new code
receiver = ["audio-decode"]

# Audio decoding (required for receiver)
audio-decode = ["dep:alac", "dep:symphonia"]

# Audio output backends (pick one or more)
audio-coreaudio = ["dep:coreaudio-rs"]     # macOS (priority)
audio-cpal = ["dep:cpal"]                   # Cross-platform fallback
audio-alsa = ["dep:alsa"]                   # Linux native

# All receiver features for full build
receiver-full = ["receiver", "audio-coreaudio", "audio-cpal"]

# Password protection (deferred)
receiver-auth = ["receiver"]
```

### Conditional Compilation

```rust
// In src/lib.rs
#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "receiver")]
pub mod receiver;

// In src/receiver/mod.rs
#[cfg(feature = "audio-coreaudio")]
pub mod output_coreaudio;

#[cfg(feature = "audio-alsa")]
pub mod output_alsa;

#[cfg(feature = "audio-cpal")]
pub mod output_cpal;
```

---

## Module Structure

### Proposed Crate Structure (Receiver Additions)

```
src/
├── lib.rs                      # Add receiver exports
├── receiver/                   # NEW: Receiver module
│   ├── mod.rs                  # Module exports
│   ├── config.rs               # ReceiverConfig
│   ├── server.rs               # AirPlayReceiver main struct
│   ├── session.rs              # Session state machine
│   ├── rtsp_handler.rs         # RTSP method handlers
│   ├── rtp_receiver.rs         # UDP receive loops
│   ├── timing.rs               # Timing sync responses
│   └── events.rs               # Receiver events
│
├── discovery/
│   ├── mod.rs
│   ├── browser.rs              # Existing
│   ├── parser.rs               # Existing
│   └── advertiser.rs           # NEW: Service advertisement
│
├── audio/
│   ├── mod.rs
│   ├── buffer.rs               # Existing (reused)
│   ├── format.rs               # Existing (reused)
│   ├── jitter.rs               # NEW: Jitter buffer
│   ├── output.rs               # NEW: Output trait
│   ├── output_coreaudio.rs     # NEW: macOS backend
│   ├── output_cpal.rs          # NEW: Cross-platform backend
│   └── output_alsa.rs          # NEW: Linux backend
│
├── protocol/
│   ├── rtsp/
│   │   ├── mod.rs
│   │   ├── codec.rs            # Existing
│   │   ├── server_codec.rs     # NEW: Server-side parsing
│   │   └── ...
│   ├── sdp/
│   │   ├── mod.rs
│   │   ├── encoder.rs          # Existing
│   │   └── parser.rs           # NEW: SDP parsing
│   └── raop/
│       ├── encryption.rs       # Existing (reused for decrypt)
│       └── ...
│
└── testing/
    ├── mock_server.rs          # Existing
    └── mock_sender.rs          # NEW: Mock AirPlay sender
```

---

## Receiver State Machine

```
                    ┌─────────────────┐
                    │     Idle        │
                    │  (advertising)  │
                    └────────┬────────┘
                             │ TCP connect
                    ┌────────▼────────┐
                    │   Connected     │
                    │ (awaiting RTSP) │
                    └────────┬────────┘
                             │ ANNOUNCE
                    ┌────────▼────────┐
                    │   Announced     │
                    │  (SDP parsed)   │
                    └────────┬────────┘
                             │ SETUP
                    ┌────────▼────────┐
                    │     Setup       │
                    │ (ports ready)   │
                    └────────┬────────┘
                             │ RECORD
                    ┌────────▼────────┐
           FLUSH ──▶│    Streaming    │◀── Audio packets
                    │  (playing)      │
                    └────────┬────────┘
                             │ TEARDOWN
                    ┌────────▼────────┐
                    │   Teardown      │
                    │ (cleanup)       │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │      Idle       │
                    └─────────────────┘
```

---

## Section Dependencies (Receiver)

```
                         ┌──────────────────┐
                         │ 34: Receiver     │
                         │ Overview (this)  │
                         └────────┬─────────┘
                                  │
           ┌──────────────────────┼──────────────────────┐
           │                      │                      │
  ┌────────▼────────┐   ┌─────────▼────────┐   ┌────────▼────────┐
  │ 35: Service     │   │ 36: RTSP Server  │   │ 42: Audio       │
  │ Advertisement   │   │ (Sans-IO)        │   │ Output          │
  └────────┬────────┘   └─────────┬────────┘   └────────┬────────┘
           │                      │                     │
           │            ┌─────────▼────────┐            │
           │            │ 37: Session      │            │
           │            │ Management       │            │
           │            └─────────┬────────┘            │
           │                      │                     │
           │            ┌─────────▼────────┐            │
           │            │ 38: SDP Parsing  │            │
           │            │ & Stream Setup   │            │
           │            └─────────┬────────┘            │
           │                      │                     │
           │            ┌─────────▼────────┐            │
           │            │ 39: RTP Receiver │            │
           │            │ Core             │            │
           │            └─────────┬────────┘            │
           │                      │                     │
           │   ┌──────────────────┼──────────────────┐  │
           │   │                  │                  │  │
  ┌────────▼───▼────┐   ┌─────────▼────────┐  ┌─────▼──▼────────┐
  │ 40: Timing      │   │ 41: Jitter       │  │                 │
  │ Synchronization │   │ Buffer           │  │                 │
  └────────┬────────┘   └─────────┬────────┘  │                 │
           │                      │           │                 │
           └──────────────────────┼───────────┘                 │
                                  │                             │
                         ┌────────▼────────┐                    │
                         │ 43: Volume &    │                    │
                         │ Metadata        │                    │
                         └────────┬────────┘                    │
                                  │                             │
                         ┌────────▼────────┐                    │
                         │ 44: Receiver    │◀───────────────────┘
                         │ Integration     │
                         └────────┬────────┘
                                  │
                         ┌────────▼────────┐
                         │ 45: Receiver    │
                         │ Testing         │
                         └─────────────────┘
```

---

## Parallel Work Streams (Receiver)

### Stream R1: Network & Discovery
- Section 35 (Service Advertisement)
- Can proceed independently

### Stream R2: Protocol Core
- Section 36 (RTSP Server) → 37 (Session) → 38 (SDP) → 39 (RTP Receiver)
- Sequential dependency

### Stream R3: Audio Pipeline
- Section 42 (Audio Output) → 41 (Jitter Buffer) → 40 (Timing)
- Can proceed in parallel with R2

### Stream R4: Testing Infrastructure
- Section 45 (Testing)
- Can start early, expand as other sections complete

### Convergence
- Section 43 (Volume/Metadata) requires R2
- Section 44 (Integration) requires R1, R2, R3
- Section 45 (Testing) validates all

---

## Comparison: Receiver vs Reference Implementations

| Feature | shairport-sync | This Implementation |
|---------|----------------|---------------------|
| Language | C | Rust |
| AirPlay 1 | Yes | Yes (goal) |
| AirPlay 2 | Yes | Client only (for now) |
| Audio backends | ALSA, PulseAudio, etc. | Trait-based, pluggable |
| Encryption | RSA + AES | RSA + AES (reuse existing) |
| Password | Yes | Deferred |
| Metadata | Yes | Yes |
| Multi-room | Yes | Future consideration |

---

## Testing Philosophy

Testing is **critical** for the receiver implementation. Each section includes:

1. **Unit Tests**: Protocol parsing, state machines, codec correctness
2. **Integration Tests**: Full RTSP/RTP exchanges with mock components
3. **Conformance Tests**: Behavior matching against shairport-sync
4. **Interoperability Tests**: Real sender compatibility (manual + documented)
5. **Network Simulation**: Packet loss, jitter, reordering scenarios
6. **Performance Tests**: Latency, throughput, buffer efficiency

See **Section 45** for comprehensive testing infrastructure.

---

## Acceptance Criteria (Overview)

- [ ] Architecture documented and approved
- [ ] Feature flags designed and documented
- [ ] Module structure defined
- [ ] Dependency graph established
- [ ] Reuse strategy identified for all components
- [ ] Testing philosophy documented
- [ ] All section documents (35-45) created

---

## Notes

- **Priority**: macOS audio output (CoreAudio) is the priority, but design for all platforms
- **Password Protection**: Design hooks now, implement later (receiver-auth feature)
- **FairPlay**: Not supported (requires Apple licensing)
- **AirPlay 2 Receiver**: Future consideration, beyond current scope
- **Reference**: Compare behavior with [shairport-sync](https://github.com/mikebrady/shairport-sync)

---

## References

- [Unofficial AirPlay Specification](https://nto.github.io/AirPlay.html)
- [OpenAirPlay Audio Spec](https://openairplay.github.io/airplay-spec/audio/)
- [shairport-sync](https://github.com/mikebrady/shairport-sync) - Reference implementation
- [RAOP Protocol Analysis](https://git.zx2c4.com/Airtunes2/about/)
