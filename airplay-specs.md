# AirPlay 1 and 2: Complete Technical Specifications

## Audio Formats

Both AirPlay 1 and 2 support four primary audio codecs:

- **PCM** — Uncompressed linear audio
- **ALAC** — Apple Lossless Audio Codec (lossless compression)
- **AAC** — Advanced Audio Codec (lossy compression)
- **AAC-ELD** — Enhanced Low Delay (real-time communication optimized)

### Sample Rates and Bit Depths

| Parameter | AirPlay 1 | AirPlay 2 |
|-----------|-----------|----------|
| Standard configuration | 16-bit/44.1 kHz (CD quality) | 16-bit/44.1 kHz (standard) |
| High-resolution support | Not supported | 24-bit/48 kHz on compatible devices (HomePod, HomePod mini) |
| Apple Music streaming | AAC 256 kbps (lossy) | AAC 256 kbps (lossy) |
| Local ALAC streaming | Lossless at 16-bit/44.1 kHz | Lossless at 16-bit/44.1 kHz or 24-bit/48 kHz (device dependent) |

## Video Formats

### Codecs and Specifications

- **Primary codec**: H.264 (MPEG-4 Part 10)
- **Secondary codec**: MPEG-4
- **Maximum resolution**: 1080p (1920×1080)
- **Frame rate**: Up to 60 fps
- **H.264 profiles**: High/Main Profile level 4.2 or lower; Baseline Profile level 3.0 with AAC-LC audio
- **MPEG-4 resolution**: Up to 640×480 at 30 fps
- **Recommended bitrate**: ~25 Mbps (actual varies with content)
- **Screen mirroring latency**: ~150 milliseconds

### Limitations

- **4K**: Not supported via AirPlay streaming
- **Dolby Atmos**: Not supported via AirPlay streaming
- **DRM-protected content**: Cannot be mirrored (Netflix, Hulu, etc.) due to HDCP restrictions
- **Lossless video**: Not supported

## Photo and Image Formats

- **Format**: JPEG
- **Use cases**: Cover artwork display, photo streaming, slideshows
- **Metadata transmission**: Optional (track title, artist, album, artwork, playback progress)
- **Slideshow support**: Native with transitions and accompanying music

## Authentication Methods

### AirPlay 1: Legacy Authentication

#### RSA Authentication (Type 1)
- **Key algorithm**: RSA-1024 with SHA-1 hash
- **Mechanism**: Asymmetric key verification
- **Session encryption**: AES-128 CTR (Counter mode)
- **Pairing**: Password-based numeric PIN (typically 4 digits)
- **Transport**: RAOP (Remote Audio Output Protocol) over UDP
- **Devices**: Primarily AirPort Express base stations

#### Audio Encryption (RAOP)
- **Algorithm**: AES-128 symmetric encryption
- **Key exchange**: RSA asymmetric encryption for session key
- **Channel**: UDP-based RTP for audio payload
- **Redundancy**: Optional RTP packet redundancy support

### AirPlay 2: HomeKit-Based Pairing

#### Transient Pairing
- **Purpose**: Devices without persistent credential storage (e.g., AirPort Express 2)
- **Setup code**: Fixed default code of **3939**
- **Protocol**: Two-step `/pair-setup` exchange
- **Key agreement**: SRP (Secure Remote Password)
- **Encryption**: ChaCha20-Poly1305 AEAD cipher for subsequent connections

#### Standard Pairing (with PIN)
- **Setup steps**: Three-step process
  1. Pair-setup: SRP6a key agreement produces shared session key
  2. Pair-verify: Confirms key material possession
  3. Subsequent connections: ChaCha20-Poly1305 encrypted communication
- **SRP specifications**:
  - 16-byte randomly generated salt
  - SHA-1 hashing for key derivation (legacy implementations)
  - SHA-512 hashing for modern implementations
  - Curve25519 for post-quantum-resistant key agreement
- **Encryption**:
  - Algorithm: ChaCha20-Poly1305 AEAD
  - Key derivation: HKDF-SHA-512
  - Separate keys for each direction:
    - Control-Write-Encryption-Key (client to device)
    - Control-Read-Encryption-Key (device to client)
  - Nonce: 64-bit counter per message

#### MFi Authentication (Type 8)
- **Purpose**: Third-party manufacturer certification
- **Mechanism**: RSA-1024 signed certificates during pairing
- **Signature**: Computed over HKDF-derived material and encrypted within `/pair-setup`
- **Key agreement**: Enhanced SRP with certificate validation

#### FairPlay Authentication (Types 3 and 5)
- **Type 3 (Standard FairPlay)**: 
  - Content key encryption: AES-128
  - Video encryption: AES-CBC per frame (H.264)
  - Audio encryption: AES-CBC per sample (protected streams)
  - Key wrapping: RSA for session-specific content keys
- **Type 5 (FairPlay SAPv2.5)**:
  - Encryption: AES-GCM (authenticated encryption)
  - Integrity verification: Included with confidentiality
  - Enhanced security over Type 3

## Pairing Methods

### AirPlay 1

| Method | Device Type | Mechanism |
|--------|------------|-----------|
| **Legacy Password Pairing** | AirPort Express | Numeric PIN presented during RTSP session setup; validated against server-stored hash |

### AirPlay 2

| Method | Device Type | Mechanism |
|--------|------------|-----------|
| **Transient Pairing** | Fixed-code devices | Code 3939; no persistent storage |
| **Standard HomeKit Pairing** | User-configurable devices | PIN display on first connection; stored in Home app |
| **QR Code Pairing** | Modern receivers | Encoded pairing information for rapid provisioning |
| **Apple TV Device Verification** | Apple TV (tvOS 10.2+) | Three-step setup with verify-step encryption |

## Protocol Stack and Transport

### Network Layers

| Protocol | Layer | Purpose | Details |
|----------|-------|---------|---------|
| **Bonjour/mDNS** | Service Discovery | Device announcement and discovery | Multicast on UDP port 5353 |
| **HTTP/HTTPS** | Application | Device metadata and configuration | Ports 80 (HTTP), 443 (HTTPS) |
| **RTSP** | Application | Media session control and commands | Ports 554 (TCP/UDP) |
| **RTP/RTCP** | Transport | Audio/video payload and quality feedback | UDP ports (content-dependent) |
| **TCP** | Transport | Control channels and interleaved RTP | Ports 5000–5001, 7000 |
| **UDP** | Transport | Real-time audio streaming and timing | Ports 7000–7011 (AirPlay 1) |

### Port Mapping

| Port | Protocol | Purpose |
|------|----------|---------|
| **80** | TCP | HTTP device discovery |
| **443** | TCP | HTTPS secure communication |
| **554** | TCP/UDP | RTSP media control |
| **5000–5001** | TCP | AirPlay control channels |
| **5353** | UDP | mDNS/Bonjour service discovery |
| **7000** | TCP | AirPlay streaming data |
| **7010–7011** | UDP | NTP time synchronization (AirPlay 1) |
| **Varies** | UDP | PTP timing channels (AirPlay 2) |

### Buffering and Latency

| Aspect | AirPlay 1 | AirPlay 2 |
|--------|-----------|----------|
| **Buffering** | ~2 seconds (fixed) | Adaptive buffering (configurable depth) |
| **Latency** | ~150 ms (optimal conditions) | ~150 ms (mirroring); <1 ms (multi-room PTP sync) |
| **Packet loss handling** | Vulnerable on congested networks | Resilient with redundancy and adaptive buffering |

## Time Synchronization

### AirPlay 1: NTP (Network Time Protocol)
- **Architecture**: Source acts as NTP server; receivers are NTP clients
- **Accuracy**: Moderate (~milliseconds)
- **Suitability**: Casual multi-room listening
- **Ports**: UDP 7010–7011

### AirPlay 2: PTP (Precision Time Protocol)
- **Architecture**: Master clock with slave synchronization
- **Accuracy**: Sub-millisecond (hardware-assisted timestamping)
- **Suitability**: Tight audio synchronization across zones
- **Fallback**: NTP if PTP unavailable
- **Feature bit**: Bit 41 (`SupportsPTP`)
- **Timing channels**: Negotiated per-session (not fixed ports)

## Device Feature Flags

AirPlay devices advertise capabilities through a **64-bit feature bitfield** in Bonjour TXT records.

### Critical Bits for Protocol Selection

| Bit | Feature | AirPlay 1/2 | Purpose |
|-----|---------|------------|---------|
| **0** | `SupportsVideo` | Both | H.264 video streaming |
| **1** | `SupportsPhoto` | Both | JPEG photo display |
| **5** | `SupportsSlideshow` | Both | Photo slideshow with transitions |
| **7** | `SupportsScreen` | Both | Screen mirroring (1080p) |
| **8** | `SupportsScreenRotate` | Both | Software-based screen rotation |
| **9** | `SupportsAudio` | Both | Audio streaming (all codecs) |
| **11** | `AudioRedundant` | Both | RTP packet redundancy for resilience |
| **12** | `FPSAPv2pt5_AES_GCM` | AP2 | FairPlay SAPv2.5 authenticated encryption |
| **14** | `Authentication4` | AP2 | FairPlay DRM support |
| **19, 20, 21** | `AudioFormat1/2/3` | AP2 | **Mandatory for AirPlay 2** |
| **23** | `Authentication1` | AP1 | RSA authentication (legacy) |
| **27** | `SupportsLegacyPairing` | AP1 | AirPort Express password pairing |
| **38** | `SupportsCoreUtilsPairingAndEncryption` | AP2 | HomeKit pairing with ChaCha20-Poly1305 |
| **40** | `SupportsBufferedAudio` | AP2 | Buffered audio mode (multi-room requirement) |
| **46** | `SupportsHKPairingAndAccessControl` | AP2 | Full HomeKit integration |
| **51** | `SupportsUnifiedPairSetupAndMFi` | AP2 | MFi authentication during pairing |

**Client behavior**: Clients prefer AirPlay 2 when bits 19 and 20 are present; fall back to AirPlay 1 if unavailable.

## Service Discovery

### Bonjour (mDNS)

- **Service type**: `_airplay._tcp.local.`
- **Record type**: PTR (service enumeration), SRV (host and port), TXT (feature flags and properties)
- **Multicast address**: 224.0.0.251:5353 (IPv4), [ff02::fb]:5353 (IPv6)
- **TTL**: 4500 seconds (standard)
- **Re-announcement**: Every 120 seconds (device presence heartbeat)

### TXT Record Fields

| Field | Example | Purpose |
|-------|---------|---------|
| `md` | `Acme Speaker` | Model/friendly name |
| `pw` | `1` | Password protection required (legacy AirPlay 1) |
| `ff` | `0x5a7ffdc2` | 64-bit feature flag bitfield |
| `sf` | `0x0` | Status flags (0=available, 1=busy) |
| `ci` | `1` | Category identifier (see [HomeKit Accessory Protocol](https://github.com/homekit2mqtt/homekit2mqtt)) |
| `vv` | `2` | Version (Bonjour version) |
| `ss` | `16` | Speaker shortcut sequence number |
| `pk` | `BASE64_STRING` | Public key for pairing (AirPlay 2) |

## Comparison: AirPlay 1 vs. AirPlay 2

| Feature | AirPlay 1 | AirPlay 2 |
|---------|-----------|----------|
| **Audio codec support** | PCM, ALAC, AAC, AAC-ELD | PCM, ALAC, AAC, AAC-ELD |
| **Maximum audio resolution** | 16-bit/44.1 kHz | 24-bit/48 kHz (device dependent) |
| **Video codec support** | H.264, MPEG-4 | H.264, MPEG-4 |
| **Maximum video resolution** | 1080p @ 60 fps | 1080p @ 60 fps |
| **Multi-room audio** | No (single zone) | Yes, with PTP sync |
| **Time synchronization** | NTP (millisecond precision) | PTP (sub-millisecond precision) |
| **Pairing mechanism** | Legacy password (AirPort Express) | HomeKit transient/standard/MFi |
| **Encryption cipher** | AES-128 CTR | ChaCha20-Poly1305 AEAD |
| **HomeKit integration** | No | Yes (full integration) |
| **QR code provisioning** | Not supported | Supported |
| **FairPlay DRM** | Basic support | SAPv2.5 with AES-GCM |
| **Buffering strategy** | Fixed (~2 seconds) | Adaptive (configurable) |
| **Control interface** | iTunes | HomeKit, native music apps |
| **Device ecosystem** | AirPort Express, older Apple TV | HomePod, Sonos, Bose, Bang & Olufsen, Apple TV 4K |

## Use Case Suitability

### AirPlay 1 Recommended For:
- Single-zone audio streaming (iTunes to AirPort Express)
- Screen mirroring to older Apple TV devices (2nd generation or later)
- Legacy AirPort Express deployments with password-protected access
- Environments where NTP-based timing is sufficient

### AirPlay 2 Recommended For:
- Multi-room audio with sub-millisecond synchronization
- HomeKit automation integration (voice control, scenes, automations)
- Modern smart speaker deployment
- Third-party device certification via MFi programs
- Large-scale provisioning via QR codes
- DRM-protected content streaming to compatible receivers

## Implementation Notes

- **Backward compatibility**: Devices supporting both versions maintain simultaneous support; clients negotiate via feature flags
- **Protocol preference**: Clients prefer AirPlay 2 (bits 19/20 present) but fall back to AirPlay 1 if unavailable
- **Encryption mandatory**: All modern AirPlay 2 devices require encryption; AirPlay 1 devices can operate unencrypted if password protection is disabled
- **Latency trade-offs**: Lower buffering improves responsiveness but increases vulnerability to packet loss; buffering trade-offs depend on network quality
