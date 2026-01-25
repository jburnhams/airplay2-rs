# AirPlay 1 Audio **Receiver**: Implementation Checklist

## Service Discovery (Bonjour/mDNS)

### Advertise RAOP Service
- [ ] Advertise `_raop._tcp.local.` service on the network[web:61]
- [ ] Construct RAOP service name as `MAC@FriendlyName` (e.g. `5855CA1AE288@My Speaker`)[web:61]
- [ ] Choose and publish RTSP listen port (commonly 5000, but can be dynamic)[web:61][web:106]

### RAOP TXT Record Fields
- [ ] Include `txtvers=1`[web:61]
- [ ] Include `ch=2` (stereo)[web:61]
- [ ] Include `cn=0,1,2,3` (PCM, ALAC, AAC variants you support)[web:61]
- [ ] Include `sr=44100` (44.1 kHz sample rate)[web:61]
- [ ] Include `ss=16` (16‑bit samples)[web:61]
- [ ] Include `tp=UDP` (RTP over UDP)[web:61]
- [ ] Include `et=0,1` (no encryption and RSA+AES‑128; add 3/5 only if you truly support FairPlay)[web:61]
- [ ] Include `pw=false` or `pw=true` depending on password requirement[web:61]
- [ ] Include `am=ModelName` (e.g. `am=MyAirplaySpeaker1`)[web:61]
- [ ] Include `vn` / `vs` (protocol and software versions) as appropriate[web:61]
- [ ] Include `sf` status flags (e.g. `0x4` when in use, etc.)[web:61]

---

## RTSP Server (Control Plane)

### RTSP Listener
- [ ] Listen on configured RTSP TCP port (e.g. 5000)[web:7]
- [ ] Accept multiple clients but enforce **one active RAOP session** at a time (unless you plan mixing)[web:103]
- [ ] Parse RTSP requests and generate compliant RTSP 1.0 responses[web:7]
- [ ] Track `CSeq` per connection and echo in responses[web:7]
- [ ] Maintain per‑session state keyed by `Session` header value[web:7]

### Required RTSP Methods (Receiver Side)
- [ ] `OPTIONS` — advertise supported methods (`ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, GET_PARAMETER, SET_PARAMETER, OPTIONS`)[web:7][web:13]
- [ ] `ANNOUNCE` — accept SDP from sender (codec, AES key/IV, latency)[web:7]
- [ ] `SETUP` — allocate UDP ports for audio, control, timing and respond with them[web:7]
- [ ] `RECORD` — start accepting and playing RTP audio; send `Audio-Latency` header[web:7]
- [ ] `FLUSH` — drop buffered audio from given `rtptime` onward, resync stream[web:7]
- [ ] `TEARDOWN` — stop playback, close ports, clear session[web:7]
- [ ] `GET_PARAMETER` — respond to keep‑alive pings (e.g. volume, progress)[web:7]
- [ ] `SET_PARAMETER` — handle volume changes, metadata, etc. if you support them[web:7]

### ANNOUNCE Handling
- [ ] Parse SDP body to determine:
  - [ ] Codec type (`L16`, `AppleLossless`, `mpeg4-generic` for AAC)[web:7]
  - [ ] `a=fmtp:` ALAC/AAC parameters (frame size, bit depth, channels, etc.)[web:7]
  - [ ] `a=rsaaeskey:` (base64) — encrypted AES key if encryption used[web:7]
  - [ ] `a=aesiv:` (base64) — AES IV if encryption used[web:7]
  - [ ] `a=min-latency:` desired latency in samples[web:7]
- [ ] Store incoming stream parameters for later RTP decoding[web:7]

### SETUP Handling (as Receiver)
- [ ] Parse `Transport` header for:
  - [ ] `RTP/AVP/UDP;unicast;mode=record`[web:7]
  - [ ] Optional client `control_port` and `timing_port`[web:7][web:92]
- [ ] Allocate:
  - [ ] Local UDP port for **audio data** (server_port)[web:7][web:92]
  - [ ] Local UDP port for **control** (control_port)[web:7][web:92]
  - [ ] Local UDP port for **timing** (timing_port)[web:7][web:92]
- [ ] Return `Transport: ...;server_port=...;control_port=...;timing_port=...` in SETUP response[web:7][web:92]
- [ ] Generate new `Session` ID and include in response[web:7]

### RECORD / FLUSH / TEARDOWN
- [ ] On `RECORD`:
  - [ ] Parse `Range: npt=0-` and `RTP-Info: seq=...,rtptime=...`[web:7]
  - [ ] Set initial expected RTP sequence and timestamp[web:7]
  - [ ] Respond with `Audio-Latency: <samples>` indicating your latency (e.g. 2205, 4410, or ~2 s)[web:7]
- [ ] On `FLUSH`:
  - [ ] Read `RTP-Info: rtptime=...` if present[web:7]
  - [ ] Drop/flush packets at or after that timestamp from jitter buffer[web:7]
  - [ ] Realign playback start to new incoming packets[web:7]
- [ ] On `TEARDOWN`:
  - [ ] Stop playback threads
  - [ ] Close UDP sockets (audio/control/timing)
  - [ ] Clear session state and mark device free (`sf` flag)[web:7][web:103]

---

## Authentication and Encryption

### Encryption Modes
- [ ] Support `et=0` (no encryption) for development / debug[web:61]
- [ ] Support `et=1` (RSA + AES‑128) for real AirPlay 1 senders[web:61][web:64]
- [ ] Decide if you **will not** support `et=3/5` (FairPlay / DRM) unless you are licensed[web:61]

### RSA + AES‑128 for RAOP
- [ ] Provide Apple‑compatible RSA public key in your reverse‑engineered certificate (if using full compatibility)[web:64]
- [ ] In `ANNOUNCE`, receive:
  - [ ] `a=rsaaeskey:` — base64‑encoded, RSA‑encrypted AES key[web:7][web:64]
  - [ ] `a=aesiv:` — base64‑encoded AES IV[web:7][web:64]
- [ ] Decrypt AES key using your RSA private key[web:64]
- [ ] Store decrypted AES key and IV for RTP packet decryption[web:64]
- [ ] Implement AES‑128 in CBC mode for audio payload decryption[web:64]

### Password Protection (`pw=true`)
- [ ] If you set `pw=true` in TXT, implement RTSP auth:
  - [ ] Respond `401 Unauthorized` with appropriate headers when credentials missing[web:61][web:91]
  - [ ] Accept Basic or Digest credentials from client (as implemented)[web:91]
  - [ ] Check password against locally stored AirPlay password[web:61]
- [ ] Provide UI/config to set and persist AirPlay password

---

## RTP Receiver and Timing

### UDP Socket Setup
- [ ] Open UDP socket for **audio data** (server_port from SETUP response)[web:7][web:92]
- [ ] Open UDP socket for **control** (control_port)[web:92]
- [ ] Open UDP socket for **timing** (timing_port)[web:92]
- [ ] Use a configurable base port and range (e.g. 6001, 6002, 6003, etc.)[web:98][web:100]

### Audio RTP Handling
- [ ] Receive RTP packets on audio data port (payload type typically 0x60)[web:100]
- [ ] Parse:
  - [ ] Version, Payload Type, Sequence Number, Timestamp, SSRC[web:92]
- [ ] Decrypt payload with AES‑128 CBC when encryption enabled[web:64]
- [ ] For ALAC/AAC:
  - [ ] Parse codec framing per `fmtp` parameters[web:7]
  - [ ] Decode to PCM 16‑bit 44.1 kHz stereo[web:64]
- [ ] Push decoded PCM into jitter/audio buffer[web:92]

### Control Port Handling
- [ ] Listen for **sync packets** from sender (payload type 0x54)[web:100]
- [ ] Parse NTP timestamps and RTP timestamps inside sync packets[web:92][web:100]
- [ ] Adjust local playout time based on sync updates (clock skew compensation)[web:103]
- [ ] Optionally handle retransmission requests (0x55) if you plan to support them as receiver (usually sender requests, receiver answers)[web:100]

### Timing Port Handling
- [ ] Respond to timing queries:
  - [ ] Timing request (payload type 0x52)[web:100]
  - [ ] Timing response (payload type 0x53)[web:100]
- [ ] Include:
  - [ ] Originate, receive, and transmit timestamps à la NTP to allow sender to compute offset/jitter[web:92]
- [ ] Maintain a local clock synced to sender within a few hundred microseconds to a few milliseconds[web:92][web:103]

### NTP‑Like Synchronization
- [ ] Implement NTP‑style offset and delay computation from timing packets[web:92]
- [ ] Maintain mapping between source clock and local clock[web:103]
- [ ] Adjust playback timing (slight resampling / drift tweaks) to keep long‑term sync[web:103]

---

## Buffering and Audio Output

### Jitter Buffer
- [ ] Implement sequence‑number‑keyed jitter buffer for incoming RTP frames[web:92]
- [ ] Target initial latency ≈ 2 s (or use `min-latency`/`Audio-Latency` values)[web:7][web:71]
- [ ] Drop packets that arrive too late to be useful[web:92]
- [ ] Allow configurable buffer size to tune for network quality[web:71]

### Packet Loss and Concealment
- [ ] Detect missing sequence numbers as packet loss[web:92]
- [ ] Optionally respond to retransmit requests (if you choose to implement that side)[web:100]
- [ ] Use basic packet‑loss concealment:
  - [ ] Zero‑fill, or
  - [ ] Repeat previous frame for short gaps[web:92]
- [ ] Log packet loss statistics for diagnostics[web:98][web:103]

### Audio Pipeline
- [ ] Decode ALAC/AAC/PCM to 16‑bit 44.1 kHz PCM[web:64]
- [ ] Send PCM to platform audio backend (ALSA/PulseAudio/CoreAudio/etc.)[web:99][web:103]
- [ ] Keep audio hardware clock in sync with computed playout schedule[web:103]
- [ ] Implement smooth start/stop and flush (fade in/out optional)[web:103]

---

## Metadata, Volume, and Control

### SET_PARAMETER / GET_PARAMETER
- [ ] Implement `SET_PARAMETER` for:
  - [ ] Volume: e.g. `volume: -15.000000` (dB relative to full scale)[web:7]
  - [ ] Metadata: artwork, track info (if you want richer UI)[web:7]
- [ ] Implement `GET_PARAMETER` for:
  - [ ] `volume` queries from client, if needed[web:7]
- [ ] Map AirPlay volume to local volume scale or purely digital attenuation[web:7]

### Metadata (Optional)
- [ ] Parse `SET_PARAMETER` with `Content-Type: application/x-dmap-tagged` or similar for DAAP‑style metadata[web:7]
- [ ] Extract title/artist/album and artwork if you want on‑device display[web:7]
- [ ] Update UI or logs with current track info[web:103]

---

## Error Handling and Robustness

### RTSP and Network Errors
- [ ] Handle malformed RTSP requests gracefully (400/501)[web:7]
- [ ] Handle auth failures with 401/403 when `pw=true`[web:91]
- [ ] Implement timeouts for RTSP commands and idle sessions[web:7]
- [ ] Tear down session on prolonged silence or missing keep‑alives[web:7][web:103]

### Session Management
- [ ] Prevent multiple senders from using the same receiver simultaneously (or implement explicit hand‑off rules)[web:103]
- [ ] Expose “busy” state via `sf` TXT field when in use[web:61]
- [ ] Allow configuration for:
  - [ ] Idle timeout
  - [ ] Allow/deny interruption by new sender[web:98]

---

## Interoperability and Testing

### Sender Compatibility
- [ ] Test with iTunes / Music on macOS as sender[web:4]
- [ ] Test with iOS devices (iPhone/iPad) on recent iOS versions (AirPlay 1 mode)[web:4]
- [ ] Test with third‑party RAOP senders (e.g. Roon/OwnTone/pyatv)[web:101][web:59]
- [ ] Confirm working with:
  - [ ] PCM
  - [ ] ALAC
  - [ ] AAC streams[web:64]

### Network Conditions
- [ ] Test on 2.4 GHz and 5 GHz Wi‑Fi under:
  - [ ] Low latency, low loss
  - [ ] Moderate loss (1–5%)
  - [ ] High jitter conditions[web:98][web:71]
- [ ] Verify behavior when:
  - [ ] Sender roams between APs
  - [ ] IP changes mid‑session
  - [ ] Multicast is filtered or rate‑limited[web:99][web:106]

### Regression / Reference
- [ ] Compare behavior with **shairport-sync** for timing and audio behavior[web:103]
- [ ] Verify sequence of RTSP messages matches unofficial spec examples[web:7][web:13]
- [ ] Ensure your implementation works with senders that insist on UDP (not TCP‑only RAOP)[web:99]

---

This checklist covers the main behaviors you need on the **receiver** side: Bonjour advertisement, RTSP server, RSA+AES handling, RTP receive/timing, buffering, and audio output, with optional password and metadata support.[web:7][web:61][web:64][web:92][web:103]
