# AirPlay 1 Support - Overview

## Introduction

This document provides an overview of adding AirPlay 1 (also known as AirTunes or RAOP - Remote Audio Output Protocol) support to the `airplay2-rs` library. AirPlay 1 is the original audio streaming protocol introduced by Apple, predating AirPlay 2's enhanced features.

## Protocol Comparison

### AirPlay 1 (RAOP) vs AirPlay 2

| Feature | AirPlay 1 (RAOP) | AirPlay 2 |
|---------|------------------|-----------|
| Service Discovery | `_raop._tcp` | `_airplay._tcp` |
| Primary Port | 49152 (varies) | 7000 |
| Authentication | RSA challenge-response | HomeKit pairing (SRP-6a, Ed25519, X25519) |
| Key Exchange | RSA-OAEP (1024-bit) | X25519 + HKDF |
| Session Encryption | AES-128 (CTR mode) | ChaCha20-Poly1305 / AES-GCM |
| Control Protocol | RTSP with SDP | RTSP with binary plist |
| Audio Transport | RTP over UDP | RTP over UDP |
| Multi-room | Not supported | Supported |
| Buffered Audio | Not supported | Supported |
| Time Sync | NTP-style timing | PTP / NTP |

### Shared Components

Both protocols share significant architectural similarities that can be leveraged:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Unified Public API                           │
│                 AirPlayClient (extended)                        │
└─────────────────────────────────┬───────────────────────────────┘
                                  │
         ┌────────────────────────┴────────────────────────┐
         │                                                 │
┌────────▼────────┐                               ┌────────▼────────┐
│   AirPlay 2     │                               │   AirPlay 1     │
│   Connection    │                               │   Connection    │
│   (HomeKit)     │                               │   (RSA Auth)    │
└────────┬────────┘                               └────────┬────────┘
         │                                                 │
         └────────────────────────┬────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────┐
│                    Shared Protocol Layer                         │
│  ┌──────────────────┐  ┌──────────────────┐  ┌────────────────┐ │
│  │  RTSP Protocol   │  │  RTP Protocol    │  │  Audio Codecs  │ │
│  │  (Sans-IO)       │  │  (Sans-IO)       │  │  (ALAC, AAC)   │ │
│  └──────────────────┘  └──────────────────┘  └────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## AirPlay 1 Protocol Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                     Application Layer                            │
│         Remote Control (DACP) │ Metadata (DAAP/DMAP)            │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                      Session Layer                               │
│                    RTSP over TCP                                 │
│  OPTIONS → ANNOUNCE → SETUP → RECORD → SET_PARAMETER → TEARDOWN │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                     Transport Layer                              │
│  ┌────────────────┐  ┌───────────────┐  ┌───────────────────┐   │
│  │  Audio Stream  │  │    Control    │  │     Timing        │   │
│  │  (RTP/UDP)     │  │   (RTP/UDP)   │  │   (NTP/UDP)       │   │
│  └────────────────┘  └───────────────┘  └───────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                     Security Layer                               │
│         RSA-OAEP Key Exchange │ AES-128-CTR Encryption          │
└─────────────────────────────────────────────────────────────────┘
                                │
┌─────────────────────────────────────────────────────────────────┐
│                    Discovery Layer                               │
│              mDNS/DNS-SD (_raop._tcp)                           │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Strategy

### Phase 1: Core Protocol Extensions

1. **RSA Authentication Module** (Section 02)
   - RSA-1024/2048 key handling
   - Challenge-response protocol
   - OAEP encryption for key exchange

2. **RAOP Service Discovery** (Section 01)
   - Parse `_raop._tcp` TXT records
   - Detect AirPlay 1 vs AirPlay 2 devices
   - Extract audio capabilities (codecs, encryption types)

3. **RTSP Extensions for RAOP** (Section 03)
   - SDP body parsing/generation
   - AirPlay 1-specific headers (`Apple-Challenge`, `Apple-Response`)
   - Legacy session management

### Phase 2: Audio Streaming

4. **RTP Enhancements** (Section 04)
   - Sync packet handling
   - Timing protocol (NTP-style)
   - Retransmission support

5. **Audio Encryption** (Section 05)
   - AES-128-CTR for audio payload
   - RSA-OAEP key encapsulation
   - IV management

### Phase 3: Extended Features

6. **Remote Control (DACP)** (Section 06)
   - Playback commands
   - Service advertisement

7. **Metadata (DAAP/DMAP)** (Section 07)
   - Track information
   - Artwork delivery
   - Progress updates

### Phase 4: Integration

8. **Unified Client API** (Section 08)
   - Protocol auto-detection
   - Shared abstractions
   - Graceful fallback

9. **Testing Infrastructure** (Section 09)
   - Mock RAOP server
   - Protocol compliance tests
   - Real device testing

## Section Dependencies

```
┌─────────────────────────────────────────────────────────────────┐
│                  Existing AirPlay 2 Foundation                   │
│  Core Types │ Crypto │ RTSP │ RTP │ mDNS │ Audio │ Connection   │
└─────────────────────────────┬───────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
┌────────▼───────┐   ┌────────▼───────┐   ┌───────▼────────┐
│ 01: RAOP       │   │ 02: RSA        │   │ 03: RTSP       │
│ Discovery      │   │ Authentication │   │ Extensions     │
└────────┬───────┘   └────────┬───────┘   └───────┬────────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
┌────────▼───────┐   ┌────────▼───────┐   ┌───────▼────────┐
│ 04: RTP        │   │ 05: Audio      │   │ 06: Remote     │
│ Enhancements   │   │ Encryption     │   │ Control        │
└────────┬───────┘   └────────┬───────┘   └───────┬────────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
                     ┌────────▼───────┐
                     │ 07: Metadata   │
                     │ (DAAP/DMAP)    │
                     └────────┬───────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
┌────────▼───────┐                       ┌────────▼───────┐
│ 08: Integration│                       │ 09: Testing    │
│ Guide          │                       │ Strategy       │
└────────────────┘                       └────────────────┘
```

## Code Reuse Analysis

### Components to Extend

| Existing Component | AirPlay 1 Extension Required |
|--------------------|------------------------------|
| `src/protocol/rtsp/` | Add SDP parsing, Apple headers |
| `src/protocol/rtp/` | Add sync packets, timing protocol |
| `src/protocol/crypto/` | Add RSA-OAEP module |
| `src/discovery/` | Parse RAOP TXT records |
| `src/audio/` | Shared (ALAC, AAC codecs) |
| `src/connection/` | Add RAOP connection state machine |

### New Components

| New Component | Purpose |
|---------------|---------|
| `src/protocol/raop/` | RAOP-specific protocol logic |
| `src/protocol/sdp/` | SDP parsing/generation |
| `src/protocol/dacp/` | Remote control protocol |
| `src/protocol/daap/` | Metadata protocol |

## Design Principles

### 1. Protocol Detection

The library should automatically detect device capabilities:

```rust
pub enum DeviceProtocol {
    /// AirPlay 2 with HomeKit pairing
    AirPlay2,
    /// AirPlay 1 with RSA authentication
    AirPlay1,
    /// Device supports both protocols
    Both { prefer: PreferredProtocol },
}

pub struct AirPlayDevice {
    // ... existing fields ...
    pub protocol: DeviceProtocol,
    pub raop_capabilities: Option<RaopCapabilities>,
}
```

### 2. Unified Connection API

```rust
impl AirPlayClient {
    /// Connect to device using appropriate protocol
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        match device.protocol {
            DeviceProtocol::AirPlay2 => self.connect_airplay2(device).await,
            DeviceProtocol::AirPlay1 => self.connect_raop(device).await,
            DeviceProtocol::Both { prefer } => {
                match prefer {
                    PreferredProtocol::AirPlay2 => self.connect_airplay2(device).await,
                    PreferredProtocol::AirPlay1 => self.connect_raop(device).await,
                }
            }
        }
    }
}
```

### 3. Shared Audio Pipeline

Both protocols share the same audio encoding and buffering:

```rust
pub trait AudioSink {
    /// Send encoded audio data
    fn send_audio(&mut self, data: &AudioPacket) -> Result<(), Error>;

    /// Get current buffer status
    fn buffer_status(&self) -> BufferStatus;
}

// Both AirPlay 1 and 2 implement this trait
impl AudioSink for RaopSession { /* ... */ }
impl AudioSink for AirPlay2Session { /* ... */ }
```

## Testing Strategy Overview

### Unit Testing

- Sans-IO protocol testing for all new components
- Property-based testing for codec roundtrips
- Known test vectors for RSA/AES operations

### Integration Testing

- Extended mock server supporting RAOP protocol
- Full session simulation tests
- Interoperability tests between AirPlay 1 and 2 modes

### Real Device Testing

- AirPort Express (1st/2nd generation)
- Older Apple TV (2nd/3rd generation)
- Third-party RAOP receivers (e.g., Shairport-sync)

## References

- [Unofficial AirPlay Protocol Specification](https://nto.github.io/AirPlay.html)
- [OpenAirPlay Specification](https://openairplay.github.io/airplay-spec/)
- [Shairport-sync](https://github.com/mikebrady/shairport-sync) - Reference RAOP implementation
- [go-airplay](https://github.com/joelgibson/go-airplay) - Go implementation
- [RAOP Wikipedia](https://en.wikipedia.org/wiki/RAOP)

## Document Index

| Section | Title | Description |
|---------|-------|-------------|
| 00 | Overview (this document) | Architecture and strategy |
| 01 | Service Discovery | RAOP mDNS/DNS-SD |
| 02 | RSA Authentication | Challenge-response protocol |
| 03 | RTSP Session | AirPlay 1 RTSP flow |
| 04 | RTP Audio Streaming | Audio transport details |
| 05 | Audio Encryption | AES encryption with RSA key exchange |
| 06 | Remote Control | DACP protocol |
| 07 | Metadata | DAAP/DMAP protocols |
| 08 | Integration Guide | Unified API design |
| 09 | Testing Strategy | Test infrastructure |
