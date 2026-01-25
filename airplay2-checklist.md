# AirPlay 2 Audio Client: Implementation Checklist

## Audio Codec Support

### Mandatory Codec Support
- [ ] **PCM** (Pulse Code Modulation) — uncompressed linear audio
- [ ] **ALAC** (Apple Lossless Audio Codec) — lossless compression
- [ ] **AAC** (Advanced Audio Codec) — lossy compression
- [ ] **AAC-ELD** (Enhanced Low Delay) — real-time communication optimized

### Sample Rate and Bit Depth Support
- [ ] **Standard**: 16-bit/44.1 kHz stereo (minimum)
- [ ] **High-resolution**: 24-bit/48 kHz (where device supports)
- [ ] Sample rate conversion/resampling if needed
- [ ] Bit depth conversion if needed
- [ ] Stereo channel configuration (mono support optional)

## Service Discovery (Bonjour/mDNS)

### Device Discovery
- [ ] Implement mDNS/Bonjour client (_airplay._tcp.local.)
- [ ] Parse PTR records (service enumeration)
- [ ] Parse SRV records (host and port resolution)
- [ ] Parse TXT records (feature flags and metadata)
- [ ] Listen on UDP port 5353 (multicast 224.0.0.251:5353)
- [ ] Handle TTL refresh (4500 seconds standard)
- [ ] Detect device presence heartbeat (re-announcements every 120 seconds)

### TXT Record Parsing
- [ ] Extract `md` (model/friendly name)
- [ ] Extract `pw` (password protection flag — legacy AP1)
- [ ] Extract `ff` (64-bit feature flag bitfield)
- [ ] Extract `sf` (status flags: 0=available, 1=busy)
- [ ] Extract `ci` (category identifier)
- [ ] Extract `vv` (version number)
- [ ] Extract `ss` (speaker shortcut sequence)
- [ ] Extract `pk` (base64-encoded public key for pairing)

### Feature Flag Interpretation
- [ ] Bit 9: `SupportsAudio` — verify audio streaming capability
- [ ] Bit 19: `AudioFormat1` — **MANDATORY for AirPlay 2**
- [ ] Bit 20: `AudioFormat2` — **MANDATORY for AirPlay 2**
- [ ] Bit 21: `AudioFormat3` — **MANDATORY for AirPlay 2**
- [ ] Bit 38: `SupportsCoreUtilsPairingAndEncryption` — HomeKit pairing support
- [ ] Bit 40: `SupportsBufferedAudio` — buffered audio mode support
- [ ] Bit 46: `SupportsHKPairingAndAccessControl` — full HomeKit integration
- [ ] Bit 51: `SupportsUnifiedPairSetupAndMFi` — MFi authentication support
- [ ] Bit 41: `SupportsPTP` — PTP timing support detection
- [ ] Protocol preference: Select AirPlay 2 if bits 19 and 20 present; fall back to AirPlay 1 if missing

## Pairing and Authentication

### HomeKit-Based Pairing Flows

#### Transient Pairing (Fixed-Code Devices)
- [ ] Implement POST `/pair-setup` endpoint handler
- [ ] Accept transient pairing code `3939`
- [ ] SRP (Secure Remote Password) key agreement
- [ ] Generate random 16-byte salt
- [ ] Support SHA-1 hashing for legacy implementations
- [ ] Support SHA-512 hashing for modern implementations
- [ ] Derive shared session key via SRP6a
- [ ] Establish pairing context without persistent storage

#### Standard HomeKit Pairing (User PIN)
- [ ] Implement three-step pairing flow:
  1. **Pair-setup**: SRP6a key agreement produces shared session key
  2. **Pair-verify**: Confirms mutual key material possession
  3. **Subsequent connections**: Use derived ChaCha20-Poly1305 keys
- [ ] SRP specifications:
  - [ ] 16-byte randomly generated salt per pairing
  - [ ] Support SHA-1 (legacy) and SHA-512 (modern) hashing
  - [ ] Curve25519 support for post-quantum-resistant key agreement
- [ ] Display PIN code to user (received from device during `/pair-setup`)
- [ ] Implement persistent pairing storage (local secure keychain/vault)
- [ ] Handle `/pair-verify` for returning devices

#### MFi Authentication (Third-Party Certification)
- [ ] Detect MFi support via feature bit 51
- [ ] RSA-1024 certificate validation during pairing
- [ ] Verify signature computed over HKDF-derived material
- [ ] Decrypt and validate certificate within `/pair-setup` flow

### Encryption and Key Derivation

#### Session Encryption (ChaCha20-Poly1305 AEAD)
- [ ] Implement ChaCha20-Poly1305 cipher suite
- [ ] Key derivation: HKDF-SHA-512
- [ ] Generate separate encryption keys for each direction:
  - [ ] **Control-Write-Encryption-Key** (client → device)
  - [ ] **Control-Read-Encryption-Key** (device → client)
- [ ] Implement 64-bit counter nonce per message
- [ ] Nonce increment: per encrypted packet
- [ ] AEAD tag validation on decryption
- [ ] Handle nonce wrap-around (counter overflow)

#### Session Key Management
- [ ] Store pairing session keys securely
- [ ] Implement session timeout and refresh
- [ ] Clear keys on logout/disconnection
- [ ] Support re-pairing if session keys compromised

## Protocol Stack and Network Transport

### RTSP (Real-Time Streaming Protocol)
- [ ] Implement RTSP 1.0 client (RFC 2326)
- [ ] Support RTSP URLs: `rtsp://device-ip:port/...`
- [ ] Implement required RTSP methods:
  - [ ] `DESCRIBE` — retrieve session description
  - [ ] `SETUP` — establish transport for streams
  - [ ] `PLAY` — start playback
  - [ ] `PAUSE` — pause playback
  - [ ] `TEARDOWN` — close session
  - [ ] `SETRATEADJUST` — adjust playback speed (if supported)
- [ ] Parse RTSP responses and headers
- [ ] Handle session identifiers (session parameter)
- [ ] Implement CSeq (command sequence) counter

### RTP/RTCP (Real-Time Transport Protocol)
- [ ] Implement RTP audio payload handling
- [ ] Support RTP header parsing (version, PT, sequence number, timestamp, SSRC)
- [ ] Handle RTP sequence number wraparound (16-bit)
- [ ] Handle RTP timestamp wraparound (32-bit)
- [ ] Buffer incoming RTP packets
- [ ] Detect packet loss via sequence number gaps
- [ ] Implement RTCP sender/receiver reports
- [ ] Calculate jitter buffer depth based on network conditions

### UDP vs. TCP Transport
- [ ] Primary: UDP for real-time audio streaming (ports 7000–7011 range)
- [ ] Fallback: TCP interleaved RTP if UDP unavailable/blocked
- [ ] Implement connection upgrade: UDP → TCP if packet loss detected
- [ ] Handle both transport modes transparently

### Port Configuration
- [ ] RTSP control: Port 554 (TCP or UDP)
- [ ] AirPlay control: Ports 5000–5001 (TCP)
- [ ] AirPlay streaming: Port 7000 (TCP)
- [ ] Dynamic port allocation: Support server-negotiated ports
- [ ] NAT/firewall traversal: Handle port mapping if needed

## Time Synchronization

### PTP (Precision Time Protocol)
- [ ] Detect PTP support via feature bit 41
- [ ] Implement PTP master-slave synchronization
- [ ] Sub-millisecond accuracy target (< 1 ms drift)
- [ ] Hardware-assisted timestamping if available
- [ ] Phase-locked loop (PLL) control
- [ ] Timing channel negotiation per RTSP session
- [ ] Dynamic port allocation (not fixed ports)

### NTP Fallback (Legacy Compatibility)
- [ ] Implement NTP client (RFC 5905)
- [ ] Fallback to NTP if PTP unavailable
- [ ] Millisecond-level accuracy (~ms precision)
- [ ] UDP ports 7010–7011 (AirPlay 1 legacy)
- [ ] NTP round-trip delay calculation
- [ ] Clock offset adjustment

### Audio Synchronization
- [ ] Maintain timestamp alignment with network timing
- [ ] Interpolate audio buffer level based on RTP timestamp
- [ ] Adjust playback speed (clock skew compensation)
- [ ] Handle clock discontinuities (NTP/PTP clock jumps)
- [ ] Sync across multiple AirPlay 2 devices (multi-room)

## Audio Buffering and Playback

### Buffer Management
- [ ] Implement adaptive buffering strategy (configurable depth)
- [ ] Minimum buffer: 500 ms (target)
- [ ] Maximum buffer: 2000 ms (AirPlay 1 compatibility)
- [ ] Detect network jitter and increase buffer dynamically
- [ ] Monitor packet loss and adjust buffer size
- [ ] Implement circular buffer for efficient memory usage
- [ ] Prevent buffer underrun/overrun

### Playback Engine
- [ ] Decode audio codec (ALAC, AAC, or PCM passthrough)
- [ ] Resampling if sample rate mismatch
- [ ] Bit depth conversion if needed
- [ ] Audio device output routing (speaker, headphones, etc.)
- [ ] Volume control (if supported by device)
- [ ] Fade-in/fade-out on start/stop
- [ ] Latency measurement: ~150 ms typical (optimal conditions)

### Packet Loss Handling
- [ ] Detect missing RTP sequence numbers
- [ ] Request retransmission (if supported)
- [ ] Silence or interpolation for lost frames
- [ ] Error concealment strategies (copy previous frame, zero-fill, etc.)
- [ ] Graceful degradation on heavy loss

## Metadata and Control

### Metadata Transmission
- [ ] Parse metadata TXT fields:
  - [ ] Track title
  - [ ] Artist name
  - [ ] Album name
  - [ ] JPEG artwork (cover art)
  - [ ] Playback progress (current position, duration)
- [ ] Transmit metadata to device (if supported)
- [ ] Update metadata during playback
- [ ] Handle metadata timing synchronization

### Playback Control
- [ ] Play command (resume playback)
- [ ] Pause command (pause at current position)
- [ ] Stop command (stop and close connection)
- [ ] Seek to position (if supported)
- [ ] Rate adjustment (speed control, if supported)
- [ ] Volume control (if device supports)
- [ ] Shuffle/repeat modes (if applicable)

### Device Status and Feedback
- [ ] Query device status (available, busy, offline)
- [ ] Handle device-initiated disconnections
- [ ] Receive playback position updates from device
- [ ] Monitor device battery status (if applicable)
- [ ] Handle device capability announcements (feature flag changes)

## Encryption and Security

### Credential Storage
- [ ] Securely store pairing credentials (local keychain/vault)
- [ ] Encrypt stored keys at rest (device-level encryption)
- [ ] Support credential deletion/unpairing
- [ ] Handle credential expiration (if applicable)
- [ ] Implement credential migration for app updates

### TLS/HTTPS (for Device Discovery and Control)
- [ ] Support HTTPS communication (port 443)
- [ ] Implement certificate validation
- [ ] Handle self-signed certificates from devices
- [ ] Support certificate pinning (optional hardening)
- [ ] Implement secure header transmission (no credentials in logs)

### Input Validation and Sanitization
- [ ] Validate RTP header fields
- [ ] Validate RTSP responses for malformed data
- [ ] Validate mDNS TXT records (prevent injection attacks)
- [ ] Validate pairing responses (prevent tampering)
- [ ] Implement rate limiting on control commands

## Error Handling and Resilience

### Connection Management
- [ ] Detect lost network connectivity
- [ ] Automatic reconnection with exponential backoff
- [ ] Handle device going offline/online
- [ ] Graceful shutdown and resource cleanup
- [ ] Connection timeout handling (60+ seconds)
- [ ] Heartbeat/keep-alive mechanism

### Error Recovery
- [ ] Handle RTP packet loss (>5% loss → increase buffer)
- [ ] Detect stalled playback and restart
- [ ] Handle incompatible device responses
- [ ] Fallback pairing mechanism if primary fails
- [ ] Log errors for debugging (without credentials)

### Timeout Handling
- [ ] RTSP command timeout: 10+ seconds
- [ ] Pairing timeout: 30+ seconds
- [ ] Device discovery timeout: 5+ seconds
- [ ] Audio streaming timeout: detect via packet arrival

## Testing and Validation

### Device Compatibility
- [ ] Test with HomePod (all generations)
- [ ] Test with HomePod mini
- [ ] Test with Apple TV 4K (various generations)
- [ ] Test with third-party AirPlay 2 devices (Sonos, Bose, etc.)
- [ ] Test with AirPort Express (if supporting legacy AP1 fallback)

### Audio Quality Testing
- [ ] PCM streaming quality verification
- [ ] ALAC lossless integrity check
- [ ] AAC quality assessment
- [ ] Sample rate accuracy (44.1 kHz vs. 48 kHz)
- [ ] Bit depth preservation (16-bit vs. 24-bit)
- [ ] Jitter and latency measurement

### Network Conditions
- [ ] Test on 2.4 GHz Wi-Fi
- [ ] Test on 5 GHz Wi-Fi (802.11ac/ax)
- [ ] Test with packet loss (1%, 5%, 10%)
- [ ] Test with high latency (>100 ms)
- [ ] Test on congested networks
- [ ] Test firewall/NAT scenarios

### Security Testing
- [ ] Verify pairing flow security
- [ ] Test encryption/decryption correctness
- [ ] Validate key derivation (HKDF)
- [ ] Test replay attack prevention (nonce handling)
- [ ] Verify no credentials in logs/diagnostics
- [ ] Test against malicious mDNS announcements

## Optional Enhancements

### Advanced Features
- [ ] Multi-room playback synchronization (multiple devices)
- [ ] Gapless playback across tracks
- [ ] HLS streaming support (HTTP Live Streaming)
- [ ] Queue management (next track, previous track)
- [ ] Favorites/bookmarks
- [ ] Equalizer support (if device supports)
- [ ] Spatial audio support (if device supports)

### User Experience
- [ ] Device discovery UI with friendly names
- [ ] Pairing UI with PIN display/entry
- [ ] Connection status indication
- [ ] Real-time playback position display
- [ ] Artwork display (album art)
- [ ] Device selection (switch between devices)
- [ ] Connection history/favorites

### Developer Tools
- [ ] Network packet logging (RTSP, RTP, mDNS)
- [ ] Timing/latency statistics
- [ ] Buffer depth visualization
- [ ] Codec detection and logging
- [ ] Feature flag parsing and display
- [ ] Debug mode for protocol details
