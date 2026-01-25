# AirPlay 2 Audio Client: Implementation Checklist

## Audio Codec Support

### Mandatory Codec Support
- [x] **PCM** (Pulse Code Modulation) — uncompressed linear audio
  - *Status*: Implemented and verified with 440Hz tone. `examples/play_pcm.rs` and `examples/connect_to_receiver.rs` support L16/44100/2.
  - *Note*: Ensure Big Endian format (verified fix).
- [ ] **ALAC** (Apple Lossless Audio Codec) — lossless compression
  - *Status*: Pending. Required for standard AirPlay 2 high-quality streaming.
- [ ] **AAC** (Advanced Audio Codec) — lossy compression
  - *Status*: Pending.
- [ ] **AAC-ELD** (Enhanced Low Delay) — real-time communication optimized
  - *Status*: Pending.

### Sample Rate and Bit Depth Support
- [x] **Standard**: 16-bit/44.1 kHz stereo (minimum)
  - *Status*: Verified. This is the format used in `examples/connect_to_receiver.rs`.
- [ ] **High-resolution**: 24-bit/48 kHz (where device supports)
- [ ] Sample rate conversion/resampling if needed
  - *Status*: Pending (using `rubato` in `Cargo.toml` but not fully integrated).
- [ ] Bit depth conversion if needed
- [ ] Stereo channel configuration (mono support optional)
  - *Status*: Implemented but **not verified** (only verified signal presence, not independent channel content).

## Service Discovery (Bonjour/mDNS)

### Device Discovery
- [x] Implement mDNS/Bonjour client (_airplay._tcp.local.)
  - *Status*: Implemented using `mdns-sd`. Verified in `examples/connect_to_receiver.rs`.
- [x] Parse PTR records (service enumeration)
- [x] Parse SRV records (host and port resolution)
- [x] Parse TXT records (feature flags and metadata)
  - *Status*: Implemented in `src/discovery/parser.rs`.
- [x] Listen on UDP port 5353 (multicast 224.0.0.251:5353)
- [x] Handle TTL refresh (4500 seconds standard)
  - *Status*: Handled by `mdns-sd` library.
- [ ] Detect device presence heartbeat (re-announcements every 120 seconds)
  - *Status*: Handled by `mdns-sd` library but **not verified** in long-running tests.

### TXT Record Parsing
- [x] Extract `md` (model/friendly name)
- [x] Extract `pw` (password protection flag — legacy AP1)
- [x] Extract `ff` (64-bit feature flag bitfield)
- [x] Extract `sf` (status flags: 0=available, 1=busy)
  - *Status*: Implemented in `src/types/device.rs`.
- [x] Extract `ci` (category identifier)
- [x] Extract `vv` (version number)
- [x] Extract `pk` (base64-encoded public key for pairing)
  - *Status*: Implemented.

### Feature Flag Interpretation
- [x] Bit 9: `SupportsAudio` — verify audio streaming capability
- [x] Bit 19: `AudioFormat1` — **MANDATORY for AirPlay 2**
- [x] Bit 20: `AudioFormat2` — **MANDATORY for AirPlay 2**
- [x] Bit 21: `AudioFormat3` — **MANDATORY for AirPlay 2**
- [x] Bit 38: `SupportsCoreUtilsPairingAndEncryption` — HomeKit pairing support
- [x] Bit 46: `SupportsHKPairingAndAccessControl` — full HomeKit integration
- [x] Bit 51: `SupportsUnifiedPairSetupAndMFi` — MFi authentication support
- [x] Bit 41: `SupportsPTP` — PTP timing support detection
  - *Status*: Feature flag parsing implemented.

## Pairing and Authentication

### HomeKit-Based Pairing Flows

#### Transient Pairing (Fixed-Code Devices)
- [x] Implement POST `/pair-setup` endpoint handler
- [x] Accept transient pairing code `3939`
  - *Status*: Implemented in `src/connection/manager.rs`.
- [x] SRP (Secure Remote Password) key agreement
- [x] Generate random 16-byte salt
- [x] Support SHA-512 hashing for modern implementations
  - *Status*: Implemented in `src/protocol/crypto/srp.rs` and `setup.rs`.
- [x] Derive shared session key via SRP6a
- [x] Establish pairing context without persistent storage
  - *Status*: Verified working.

#### Standard HomeKit Pairing (User PIN)
- [x] Implement three-step pairing flow:
  1. **Pair-setup**: SRP6a key agreement produces shared session key
  2. **Pair-verify**: Confirms mutual key material possession
  3. **Subsequent connections**: Use derived ChaCha20-Poly1305 keys
- [x] SRP specifications:
  - [x] 16-byte randomly generated salt per pairing
  - [x] Curve25519 support for post-quantum-resistant key agreement
    - *Status*: Used in Pair-Verify (M2/M3).
- [ ] Display PIN code to user (received from device during `/pair-setup`)
  - *Note*: Currently hardcoded/CLI driven. Protocol support exists but UI/callback **not verified**.
- [x] Implement persistent pairing storage (local secure keychain/vault)
  - *Status*: Implemented `FileStorage` in `src/protocol/pairing/storage.rs`.
- [ ] Handle `/pair-verify` for returning devices
  - *Status*: Implemented in `persistent_pairing.rs` but **not verified** (test run timed out before reconnect phase).

#### MFi Authentication (Third-Party Certification)
- [x] Detect MFi support via feature bit 51
- [ ] RSA-1024 certificate validation during pairing
  - *Status*: Implemented in `src/protocol/pairing/auth_setup.rs` but **not verified** (Python receiver uses OpenAirplay).
- [ ] Verify signature computed over HKDF-derived material
- [ ] Decrypt and validate certificate within `/pair-setup` flow

### Encryption and Key Derivation

#### Session Encryption (ChaCha20-Poly1305 AEAD)
- [x] Implement ChaCha20-Poly1305 cipher suite
  - *Status*: `src/protocol/crypto/chacha.rs` using `chacha20poly1305` crate.
- [x] Key derivation: HKDF-SHA-512
  - *Status*: `src/protocol/crypto/hkdf.rs`.
- [x] Generate separate encryption keys for each direction:
  - [x] **Control-Write-Encryption-Key** (client → device)
  - [x] **Control-Read-Encryption-Key** (device → client)
- [x] Implement 64-bit counter nonce per message
  - *Status*: Implemented in `EncryptedChannel`.
- [x] Nonce increment: per encrypted packet
- [x] AEAD tag validation on decryption

#### Session Key Management
- [x] Store pairing session keys securely
  - *Status*: In memory `SessionKeys` struct.
- [ ] Implement session timeout and refresh
- [ ] Clear keys on logout/disconnection
  - *Status*: Implemented but **not verified**.

## Protocol Stack and Network Transport

### RTSP (Real-Time Streaming Protocol)
- [x] Implement RTSP 1.0 client (RFC 2326)
- [x] Support RTSP URLs: `rtsp://device-ip:port/...`
- [x] Implement required RTSP methods:
  - [x] `SETUP` — establish transport for streams
  - [x] `PLAY` — start playback
  - [x] `PAUSE` — pause playback
  - [x] `TEARDOWN` — close session
  - [x] `RECORD` - for audio data
  - [ ] `SET_PARAMETER` - for volume/metadata
    - *Status*: Implemented but **not verified**.
- [x] Parse RTSP responses and headers
- [x] Handle session identifiers (session parameter)
- [x] Implement CSeq (command sequence) counter

### RTP/RTCP (Real-Time Transport Protocol)
- [x] Implement RTP audio payload handling
- [x] Support RTP header parsing (version, PT, sequence number, timestamp, SSRC)
- [x] Handle RTP sequence number wraparound (16-bit)
- [x] Handle RTP timestamp wraparound (32-bit)
  - *Status*: Basic implementation in `src/protocol/rtp/packet.rs`.
- [ ] Buffer incoming RTP packets
- [ ] Detect packet loss via sequence number gaps
- [ ] Implement RTCP sender/receiver reports
  - *Status*: Pending.

### UDP vs. TCP Transport
- [x] Primary: UDP for real-time audio streaming
  - *Status*: Using UDP sockets in `ConnectionManager`.
- [ ] Fallback: TCP interleaved RTP if UDP unavailable/blocked
- [ ] Implement connection upgrade: UDP → TCP if packet loss detected

### Port Configuration
- [x] AirPlay streaming: Port 7000 (TCP)
- [ ] Dynamic port allocation: Support server-negotiated ports
  - *Status*: Parsing `Transport` header ports implemented but **not verified** (confirmed usage of dynamic ports).

## Time Synchronization

### PTP (Precision Time Protocol)
- [x] Detect PTP support via feature bit 41
- [ ] Implement PTP master-slave synchronization
  - *Status*: Implemented partially (sockets opened) but **not verified** for accuracy/sync against receiver.
- [ ] Timing channel negotiation per RTSP session

### NTP Fallback (Legacy Compatibility)
- [ ] Implement NTP client (RFC 5905)
- [ ] Fallback to NTP if PTP unavailable

## Audio Buffering and Playback

### Buffer Management
- [ ] Implement adaptive buffering strategy (configurable depth)
  - *Status*: Basic buffering in `PcmStreamer` implemented but **not verified** with different depths.
- [ ] Prevent buffer underrun/overrun
  - *Status*: **Not verified**.

### Playback Engine
- [x] Decode audio codec (PCM passthrough)
- [ ] Resampling if sample rate mismatch
- [ ] Volume control (if supported by device)
  - *Status*: Basic volume control implemented but **not verified**.

## Metadata and Control

### Playback Control
- [x] Play command (resume playback)
- [ ] Pause command (pause at current position)
  - *Status*: Implemented but **not verified** (pause/resume cycle not tested).
- [x] Stop command (stop and close connection)
- [ ] Volume control (if device supports)
  - *Status*: `set_volume` implemented in client but **not verified** against receiver logs.

### Device Status and Feedback
- [x] Query device status (available, busy, offline)
  - *Status*: Connection state tracking implemented.

## Encryption and Security

### Credential Storage
- [x] Securely store pairing credentials (local keychain/vault)
  - *Status*: File-based storage implemented.
- [ ] Encrypt stored keys at rest (device-level encryption)
  - *Note*: Using plain JSON for now, encryption pending.

### Input Validation and Sanitization
- [x] Validate RTSP responses for malformed data
- [x] Validate mDNS TXT records (prevent injection attacks)
- [x] Validate pairing responses (prevent tampering)

## Error Handling and Resilience

### Connection Management
- [ ] Detect lost network connectivity
  - *Status*: **Not verified**.
- [ ] Automatic reconnection with exponential backoff
- [x] Graceful shutdown and resource cleanup
- [x] Connection timeout handling (60+ seconds)

## Testing and Validation

### Device Compatibility
- [x] Test with third-party AirPlay 2 devices (Python Receiver)
  - *Status*: Verified.

### Audio Quality Testing
- [x] PCM streaming quality verification
  - *Status*: Verified 440Hz tone.
- [x] Bit depth preservation (16-bit)
  - *Status*: Verified.

### Security Testing
- [x] Verify pairing flow security
- [x] Test encryption/decryption correctness
- [x] Validate key derivation (HKDF)
  - *Status*: Verified against Python receiver.
