# AirPlay 2 Audio **Receiver**: Implementation Checklist

## 1. Service Discovery (Bonjour / mDNS)

### `_airplay._tcp` Advertisement
- [ ] Advertise `_airplay._tcp.local.` service on the LAN[web:61][web:113]
- [ ] Choose and publish RTSP listen port (dynamic or fixed, e.g. 7000)[web:32]
- [ ] Use a stable instance name (e.g. `My Speaker`) and optional MAC‑like suffix if you also expose RAOP[web:113]

### TXT Record (Minimal, then extended via `/info`)
- [ ] Include base TXT keys for AirPlay 2:
  - [ ] `txtvers=1` (TXT format version)[web:61]
  - [ ] `ch=2` (stereo)[web:61]
  - [ ] `cn=0,1,2,3` (LPCM, ALAC, AAC, AAC‑ELD as supported)[web:32][web:113]
  - [ ] `sr=44100` (44.1 kHz)[web:113]
  - [ ] `ss=16` (16‑bit)[web:113]
  - [ ] `et=4` (ChaCha20‑Poly1305 AEAD for AirPlay 2)[web:111][web:113]
  - [ ] `pw=0` or `pw=1` depending on password requirement[web:61]
  - [ ] `fv=2` / `vv` / `am` / `md` with version, model and name as desired[web:61][web:113]
  - [ ] Feature bits (`ft`/`ff`) including:
    - [ ] `SupportsAudio` / AirPlay 2 audio formats (bits 9, 19, 20, 21)[web:113]
    - [ ] `SupportsBufferedAudio` (bit 40)[web:113]
    - [ ] `SupportsPTP` (bit 41) if you support PTP timing[web:68][web:113]
    - [ ] `SupportsCoreUtilsPairingAndEncryption` (bit 38)[web:19][web:113]
    - [ ] `SupportsHKPairingAndAccessControl` (bit 46)[web:19][web:113]
- [ ] If you keep TXT minimal, ensure `/info` returns extended capabilities as binary plist (e.g. `txtAirPlay`)[web:32]

---

## 2. RTSP/HTTP Server (Control Plane)

### General RTSP/HTTP Behaviour
- [ ] Listen on the advertised AirPlay TCP port (e.g. 7000)[web:32]
- [ ] Accept RTSP‑like requests using HTTP‑style verbs and paths:
  - [ ] `GET /info`
  - [ ] `POST /pair-setup`
  - [ ] `POST /pair-verify`
  - [ ] `POST /fp-setup` (if FairPlay supported)
  - [ ] `POST /auth-setup` (auth 1/4 if applicable)
  - [ ] `POST /command`
  - [ ] `POST /feedback` (heartbeat)
  - [ ] `POST /audioMode`
  - [ ] `SETUP` (multiple phases)
  - [ ] `SET_PARAMETER`
  - [ ] `GET_PARAMETER`
  - [ ] `FLUSH`
  - [ ] `TEARDOWN`[web:32][web:113]
- [ ] Handle RTSP/HTTP headers including `CSeq`, `Session`, `Content-Length`, `Content-Type`[web:32]
- [ ] Support request/response bodies encoded as **binary plist** for most control endpoints[web:32][web:113]

### `/info` Handling
- [ ] On `GET /info`, return binary plist with:
  - [ ] AirPlay version, device model, name[web:32]
  - [ ] `features` bitfield (same as TXT but richer)[web:113]
  - [ ] Supported audio codecs and formats (`audioFormats`)[web:32][web:113]
  - [ ] Initial volume (`initialVolume`, dB from ‑144 to 0)[web:32]
  - [ ] Timing capabilities (`timingProtocol`, PTP vs NTP)[web:32][web:113]
- [ ] Support follow‑up `/info` with qualifier requests (e.g. `txtAirPlay`)[web:32]

---

## 3. HomeKit / HAP Pairing and Encryption

### HomeKit‑Based Pairing (HKP)
- [ ] Implement **pair‑setup** (`POST /pair-setup`) using SRP (HAP‑style)[web:19]
  - [ ] Generate SRP salt and verifier
  - [ ] Use SRP6a with appropriate group and hash (per HAP spec)[web:19]
- [ ] Implement **pair‑verify** (`POST /pair-verify`) to:
  - [ ] Derive shared secret
  - [ ] Verify client’s proof
  - [ ] Establish secure session keys[web:19]
- [ ] Persist HAP pairing records (client identifiers + long‑term keys) securely[web:19]
- [ ] Support **transient** vs **standard** pairing semantics if you expose them in feature bits[web:19][web:113]

### Session Encryption for Control/Command
- [ ] After successful pairing, wrap RTSP/HTTP control traffic in encrypted frames:
  - [ ] Use HAP‑style framing: `N:n_bytes:tag` with ChaCha20‑Poly1305[web:19]
  - [ ] Derive read/write keys from shared secret via HKDF (SHA‑512)[web:19][web:111]
  - [ ] Maintain independent counters/nonces per direction[web:111]
- [ ] Ensure all `/command`, `/feedback`, `/audioMode`, and AirPlay 2 `SETUP` bodies are encrypted once paired[web:32][web:109]

---

## 4. AirPlay 2 SETUP Phases (Channels)

### Phase 1: Time & Event Channels
- [ ] Handle first `SETUP` with binary plist body indicating:
  - [ ] `timingProtocol` (PTP or NTP)[web:32][web:68]
  - [ ] `ekey`, `eiv`, `et` for control channel encryption (if used)[web:32]
  - [ ] `timingPeerInfo` / `timingPeerList` for PTP multi‑peer setups[web:32][web:68]
- [ ] Allocate:
  - [ ] **Event channel** (TCP): open a server socket and return port in response[web:32]
  - [ ] **Timing channel** port if using NTP; omit if using PTP only[web:32][web:68]
- [ ] Ensure event channel is accepted/connected before proceeding; otherwise abort further SETUP[web:32]

### Phase 2: Control and Data Channels (Audio)
- [ ] Handle second `SETUP` with audio configuration plist including:
  - [ ] `audioFormat` (ALAC/AAC/PCM encoded as bitfield)[web:32][web:111]
  - [ ] `ct` (compression type: 1=LPCM, 2=ALAC, 4=AAC, 8=AAC‑ELD)[web:32]
  - [ ] `shk` (shared encryption key for audio)[web:32][web:111]
  - [ ] Latency parameters, stream IDs, etc.[web:32]
- [ ] Allocate:
  - [ ] **Control channel** (UDP or TCP) for RTCP‑like packets[web:32]
  - [ ] **Data channel** (UDP or TCP) for RTP media[web:32][web:111]
- [ ] Return `controlPort` and `dataPort` in the SETUP response plist[web:32]

---

## 5. RTP, Encryption, and Timing (Audio)

### RTP Data Channel
- [ ] Receive RTP packets on **dataPort**[web:111]
- [ ] Parse RTP header: version, payload type, sequence number, timestamp, SSRC[web:111]
- [ ] Decrypt RTP payload using **ChaCha20‑Poly1305 AEAD** with:
  - [ ] Session key from `shk` (plus HKDF where applicable)[web:111]
  - [ ] Packet‑specific nonce/counter[web:111]
  - [ ] Integrity check via Poly1305 tag[web:111]
- [ ] Decode codec (LPCM/ALAC/AAC/AAC‑ELD) to PCM 44.1 kHz stereo[web:32][web:113]

### Control/RTCP‑Like Channel
- [ ] Receive control packets (RTCP‑style) on **controlPort**[web:32]
- [ ] Track:
  - [ ] Sender reports (sequence, timestamp, wall‑clock)[web:32]
  - [ ] Receiver feedback as needed (if you support reporting)[web:32]
- [ ] Use control data to refine sync and buffer management[web:32][web:112]

### Timing / Sync (PTP or NTP)
- [ ] Implement **PTP** if advertising `SupportsPTP`:
  - [ ] Participate as PTP slave to a master (typically sender or dedicated clock)[web:68][web:113]
  - [ ] Use hardware or software timestamping to achieve sub‑millisecond sync[web:68]
  - [ ] Use event channel / timingPeerInfo to join multi‑room clock group[web:32][web:68]
- [ ] Implement **NTP‑style** timing fallback if `timingProtocol=NTP`:
  - [ ] Use timing channel messages to compute offset/jitter[web:32]
  - [ ] Maintain mapping between sender time and local clock[web:32]
- [ ] Adjust playout (drift correction, resampling) to keep streams in sync across rooms[web:113][web:114]

---

## 6. Buffering, Latency, and Playback

### Buffered Audio (Multi‑Room)
- [ ] Implement **buffered audio** with configurable depth (feature bit 40)[web:113]
- [ ] Accept and honour latency fields provided in SETUP/control metadata[web:32]
- [ ] Target stable, deterministic latency for multi‑room sync rather than minimal latency[web:113][web:114]
- [ ] Maintain per‑stream jitter buffer keyed by RTP sequence number and timestamp[web:111]

### Start/Stop and FLUSH
- [ ] Implement `FLUSH` to:
  - [ ] Drop buffered audio at/after specified RTP timestamp (from `RTP-Info`/plist)[web:32]
  - [ ] Realign playback pointer to new content[web:32]
- [ ] Ensure pause/resume semantics follow AirPlay 2 expectations (buffer remains, but audio stops immediately on pause)[web:112]
- [ ] On `TEARDOWN`, cleanly stop playback, close channels and free resources[web:32]

### Audio Output
- [ ] Map decoded PCM to platform audio engine (CoreAudio/ALSA/PulseAudio/etc.)
- [ ] Keep audio output clock aligned to network time via drift correction[web:113]
- [ ] Implement smooth fade in/out on connect/disconnect/flush for good UX[web:117]

---

## 7. Metadata, Volume, and Control

### `SET_PARAMETER` / `GET_PARAMETER`
- [ ] Support `SET_PARAMETER` with these content types[web:32]:
  - [ ] `text/parameters` — e.g. `volume: N` and `progress: X/Y/Z`[web:32]
  - [ ] `image/jpeg` — artwork image data[web:32]
  - [ ] `application/x-dmap-tagged` — DAAP/now‑playing metadata[web:32]
- [ ] Adjust device volume based on `volume` (‑144 to 0 dB)[web:32]
- [ ] Optionally track and expose playback progress (`progress: start/current/end`)[web:32]
- [ ] If you support `GET_PARAMETER`, respond to volume queries appropriately[web:32]

### `/command` and `/feedback`
- [ ] Implement `/command` to handle:
  - [ ] Play/pause/seek commands where applicable[web:32]
  - [ ] Grouping/multi‑room related commands if you choose to support them[web:32][web:113]
- [ ] Implement `/feedback` as heartbeat endpoint:
  - [ ] Accept periodic pings from sender
  - [ ] Use as liveness check and to maintain session state[web:32]

---

## 8. Security, Credentials, and Policy

### Credential Storage
- [ ] Securely store HAP pairing records and derived keys (OS keychain/secure storage)[web:19]
- [ ] Support unpairing/reset of AirPlay 2 pairings via configuration/UI[web:19][web:110]
- [ ] Never log raw keys, nonces, or decrypted content

### TLS / HTTPS (if used)
- [ ] If you expose HTTPS endpoints, provide valid server certificate or handle self‑signed flows[web:113]
- [ ] Enforce certificate verification on any outbound HTTPS calls (e.g. configuration services)

### Access Control
- [ ] Honor HomeKit access control bits if you integrate with HomeKit[web:19]
- [ ] Optionally expose allow‑list / deny‑list for senders (MAC/IP/DACP ID)[web:59]
- [ ] Provide password option (`pw=1`) as legacy fallback alongside HAP pairing[web:61][web:116]

---

## 9. Error Handling and Robustness

### RTSP / Control Errors
- [ ] Validate binary plist bodies and handle malformed data gracefully[web:32]
- [ ] Return appropriate RTSP/HTTP status codes (404, 400, 401, 403, 500) as needed[web:32][web:110]
- [ ] Tear down sessions on repeated parse/decrypt failures (e.g. bad auth tag)[web:111]
- [ ] Implement idle and keep‑alive timeouts for control/event channels[web:32]

### Network Resilience
- [ ] Handle network changes (IP, Wi‑Fi roaming) gracefully where possible[web:118]
- [ ] Detect high loss/jitter and adapt buffer depth if feasible[web:113]
- [ ] Log per‑session stats: packet loss, jitter, latency, resync events[web:115][web:116]

---

## 10. Interoperability and Testing

### Sender Compatibility
- [ ] Test with:
  - [ ] iOS (recent versions) as sender using AirPlay 2[web:113]
  - [ ] macOS Music/TV apps with AirPlay 2[web:113]
  - [ ] HomePod / HomePod mini as part of gr
