# AirPlay 1 Audio Client: Implementation Checklist

## Audio and Latency

### Audio Format Support
- [ ] Support **PCM** 16‑bit stereo at 44.1 kHz (decode or passthrough).
- [ ] Support **ALAC** decode (if you want lossless end‑to‑end).
- [ ] Support **AAC** decode (common from iOS/macOS).
- [ ] Handle stream as 2‑channel stereo (downmix from >2 channels if needed).

### Latency and Jitter
- [ ] Implement default **2 s audio latency** (88200 frames at 44.1 kHz).
- [ ] Respect `min-latency` and `Audio-Latency` SDP/RTSP attributes.
- [ ] Maintain a jitter buffer sized around 2 s, adjustable at runtime.
- [ ] Monitor buffer level and adapt playback start/flush points.

## Service Discovery (Bonjour/mDNS)

### Device Discovery
- [ ] Implement mDNS/Bonjour client for `_raop._tcp.local.` and/or `_airplay._tcp.local.`.
- [ ] Parse PTR → SRV → TXT records to discover RAOP endpoints.
- [ ] Resolve hostname and port from SRV record.
- [ ] Listen on UDP 5353, multicast 224.0.0.251 (IPv4) / ff02::fb (IPv6).
- [ ] Handle TTL and re‑announcements to track availability.

### TXT Record Parsing (RAOP / AirPlay 1)
- [ ] Parse `txt` for:
  - [ ] `tp` (transport, e.g. `UDP`).
  - [ ] `et` (encryption type: 0/1/3/4).
  - [ ] `cn` (codecs supported, e.g. `0,1,2,3`).
  - [ ] `sr` (sample rate, e.g. `44100`).
  - [ ] `ss` (sample size, e.g. `16`).
  - [ ] `ch` (channels, e.g. `2`).
  - [ ] `pw` (password flag; `1` means authentication required).
  - [ ] `da` / `vn` / `am` (device info as needed).
- [ ] Decide capability match (codec, sample rate) before connecting.

## Authentication and Encryption (RAOP / AirTunes)

### Encryption Modes
- [ ] Support **no encryption** (`et=0`) for unprotected endpoints.
- [ ] Support **RSA-based key exchange + AES‑128** (`et=1`) for AirPort Express‑style devices.
- [ ] Optionally support **FairPlay/MFi** (`et=3/4`) only if you have keys/certs.

### RSA + AES‑128 Flow
- [ ] During RTSP `ANNOUNCE`, generate random 16‑byte AES session key and 16‑byte IV.
- [ ] Encrypt AES key with server’s RSA public key.
- [ ] Include `rsaaeskey` and `aesiv` in SDP body of `ANNOUNCE`.
- [ ] Use AES‑128 in CBC mode for audio payload encryption, as per RAOP.
- [ ] Keep session key/IV for the lifetime of the RTSP session.

### Password Authentication (Legacy AirPlay 1)
- [ ] When `pw=1` in TXT record, prompt user for AirPlay password.
- [ ] Implement legacy password challenge/response (RAOP auth) if targeting AirPort Express.
- [ ] Retry on `RTSP 401` or `403` with correct credentials.

## RTSP Session (Control Plane)

### RTSP Basics
- [ ] Open TCP connection to RAOP host:port from SRV record.
- [ ] Implement RTSP 1.0 client with:
  - [ ] `OPTIONS`
  - [ ] `ANNOUNCE`
  - [ ] `SETUP`
  - [ ] `RECORD`
  - [ ] `FLUSH`
  - [ ] `TEARDOWN`
  - [ ] `SET_PARAMETER`
  - [ ] `GET_PARAMETER` (for keep‑alive).
- [ ] Maintain `CSeq` counter and parse `Session` header.
- [ ] Send periodic `OPTIONS`/`GET_PARAMETER` as keep‑alive.

### SDP in ANNOUNCE
- [ ] Generate SDP describing audio stream, e.g.:
  - [ ] `m=audio` with RTP/AVP payload type 96.
  - [ ] `a=rtpmap:96 AppleLossless` or `L16` depending on codec.
  - [ ] `a=fmtp:` with ALAC parameters if used.
  - [ ] `a=rsaaeskey` and `a=aesiv` when encryption enabled.
  - [ ] `a=min-latency` for desired latency.
- [ ] Include codec, sample rate, channels consistent with TXT record.

### SETUP / RECORD / FLUSH / TEARDOWN
- [ ] In `SETUP`, negotiate **RTP over UDP**:
  - [ ] Provide `control_port` and `timing_port` in `Transport` header.
  - [ ] Parse server response for `server_port`, `control_port`, `timing_port`.
- [ ] On `RECORD`:
  - [ ] Send `Range: npt=0-` to start playback.
  - [ ] Include `RTP-Info: seq=...,rtptime=...` for sync if acting as source.
- [ ] Implement `FLUSH` to:
  - [ ] Drop buffered audio from a given `rtptime` onwards.
  - [ ] Reset latency and resume.
- [ ] On `TEARDOWN`, close ports and cleanup state.

## RTP / Timing / Sync

### RTP Audio Stream
- [ ] Send audio frames as RTP packets to `server_port` using PT 96 (or negotiated).
- [ ] Properly fill:
  - [ ] Version, Payload Type, Sequence Number, Timestamp, SSRC.
- [ ] Increment timestamp by **frames per packet** (e.g. 352 or 882 samples) at 44.1 kHz.
- [ ] Handle sequence number wrap‑around (16‑bit) and timestamp wrap‑around (32‑bit).

### Control and Timing Packets
- [ ] Open UDP socket for **control_port** and **timing_port** from SETUP.
- [ ] Implement sync packets to `control_port`:
  - [ ] Send roughly once per second.
  - [ ] Include current NTP time and next RTP timestamp.
- [ ] Implement timing packets on `timing_port`:
  - [ ] Queries and replies with three NTP timestamps.
  - [ ] Use responses to estimate clock offset/jitter.

### NTP-Based Time Sync
- [ ] Maintain local clock synchronized to sender via NTP‑like timing exchange.
- [ ] Use NTP offset to place RTP timestamps into a consistent time domain.
- [ ] Adjust local playback clock (slight resampling or drift correction) to keep in sync.

## Buffering and Playback

### Buffer Strategy
- [ ] Pre‑buffer audio up to negotiated/typical latency (~2 s).
- [ ] Maintain a jitter buffer keyed by RTP sequence number.
- [ ] Drop or conceal late packets beyond buffer window.
- [ ] Support dynamic buffer resizing on poor networks.

### Packet Loss Handling
- [ ] Detect gaps in sequence numbers as packet loss.
- [ ] Optionally implement retransmit requests using RAOP retransmit packets (if supported).
- [ ] If retransmit not supported/too late, use:
  - [ ] Silence insertion or previous‑frame repeat.
  - [ ] Simple concealment rather than hard glitches.

### Audio Output
- [ ] Decode ALAC/AAC to 16‑bit 44.1 kHz PCM if needed.
- [ ] Route PCM to platform audio API (CoreAudio/ALSA/PulseAudio/etc.).
- [ ] Implement volume control via `SET_PARAMETER` / `volume` if you support remote volume semantics.
- [ ] Smooth fade in/out on start/stop and seek.

## Optional AirPlay 1 Extras (Video/Photo)

- [ ] Support H.264 RTP video stream (payload type 97) if you want screen/video.
- [ ] Handle JPEG photo push via HTTP/RTSP commands if you care about photo display.
- [ ] Respect `SupportsVideo` / `SupportsPhoto` feature flags if parsing `_airplay._tcp`.

## Error Handling and Robustness

- [ ] Implement RTSP timeouts and retry with exponential backoff.
- [ ] Gracefully handle `401/403` (auth failures) and `500` responses.
- [ ] Detect network loss and auto‑teardown the RTSP session.
- [ ] Clean up UDP sockets and buffers on any error/TEARDOWN.
- [ ] Log RTSP/RTP/mDNS events with optional verbose debugging.

## Interop and Testing

- [ ] Test against **AirPort Express** (original AirPlay 1 reference).
- [ ] Test against older **Apple TV** models in RAOP mode.
- [ ] Verify:
  - [ ] 2 s latency behavior.
  - [ ] Correct sync after long playback (no drift).
  - [ ] Correct behavior when Wi‑Fi is lossy (1–5% loss).
  - [ ] Password‑protected and open devices.
