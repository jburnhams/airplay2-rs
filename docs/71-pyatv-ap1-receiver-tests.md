# Section 71: AP1 Receiver vs pyatv — Tests

## Dependencies
- **Section 69**: pyatv Setup, Driver Scripts & Client Wrapper
- **Section 70**: AP2 Receiver vs pyatv (for `ReceiverHarness` and `FileAudioSink`)
- **Section 65**: Audio Verification & Analysis Framework
- `airplay1-receiver-checklist.md` — checklist items verified by these tests

## Overview

This section defines integration tests for our AirPlay 1 (RAOP) receiver, validated by using pyatv as an external RAOP client. pyatv connects to our receiver using the RAOP protocol, streams audio via the standard ANNOUNCE → SETUP → RECORD flow, and we verify the received audio output.

These tests are critical because RAOP is significantly different from AirPlay 2 at the protocol level — SDP-based negotiation instead of binary plist, RSA+AES encryption instead of HomeKit pairing, and NTP timing instead of PTP. Our universal receiver must handle both protocols correctly.

## Key Differences from AP2 Tests (Section 70)

| Aspect | AP2 (Section 70) | AP1/RAOP (This Section) |
|---|---|---|
| Service type | `_airplay._tcp` | `_raop._tcp` |
| Pairing | HomeKit (SRP/pair-setup) | None or password-based |
| Encryption | ChaCha20-Poly1305 | RSA + AES-128-CBC |
| Session setup | Binary plist SETUP phases | SDP in ANNOUNCE + RTSP SETUP |
| Timing | PTP | NTP-like timing packets |
| Audio key exchange | In SETUP phase 2 (`shk`) | In ANNOUNCE SDP (`rsaaeskey`, `aesiv`) |
| Metadata | Binary plist / DMAP | DAAP/DMAP via SET_PARAMETER |

---

## Tasks

### 71.1 RAOP Service Advertisement Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Service Discovery → Advertise RAOP Service
- `airplay1-receiver-checklist.md` → Service Discovery → RAOP TXT Record Fields

Our universal receiver must advertise both `_airplay._tcp` and `_raop._tcp` services when both protocols are enabled.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T1 | `test_ap1_receiver_advertises_raop` | Start receiver, verify `_raop._tcp` service appears | pyatv `discover` finds RAOP service |
| 71-T2 | `test_ap1_receiver_raop_txt_records` | Check TXT records of RAOP advertisement | `txtvers=1`, `ch=2`, `cn=0,1`, `sr=44100`, `ss=16`, `tp=UDP`, `et=0,1` |
| 71-T3 | `test_ap1_receiver_raop_name_format` | Verify RAOP service name is `MAC@FriendlyName` format | Name matches expected pattern |
| 71-T4 | `test_ap1_receiver_dual_advertisement` | Verify both `_airplay._tcp` and `_raop._tcp` advertised | pyatv discovers both service types |
| 71-T5 | `test_ap1_receiver_password_flag` | Configure password, verify `pw=true` in TXT record | TXT `pw=true` or `pw=1` |

**Implementation notes:**
- Use `ReceiverHarness` from Section 70 with `enable_ap1: true`.
- RAOP TXT records are different from AP2 TXT records — they use a different field set.
- Reference: `src/discovery/advertiser.rs` for mDNS advertisement, `src/types/raop.rs` for TXT record construction.
- The service name format `MAC@Name` is RAOP-specific; AP2 just uses the friendly name.

---

### 71.2 RTSP Server Tests (RAOP Mode)

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → RTSP Server → RTSP Listener, Required RTSP Methods, ANNOUNCE Handling, SETUP Handling

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T6 | `test_ap1_receiver_options` | pyatv sends OPTIONS, verify our receiver responds with supported methods | Response includes `ANNOUNCE, SETUP, RECORD, FLUSH, TEARDOWN, SET_PARAMETER, GET_PARAMETER` |
| 71-T7 | `test_ap1_receiver_announce_parsing` | pyatv sends ANNOUNCE with SDP body, verify our receiver parses it | Receiver extracts codec, sample rate, AES key/IV from SDP |
| 71-T8 | `test_ap1_receiver_setup_transport` | pyatv sends SETUP, verify our receiver allocates ports and responds | Response `Transport` header contains `server_port`, `control_port`, `timing_port` |
| 71-T9 | `test_ap1_receiver_record` | pyatv sends RECORD, verify our receiver begins accepting RTP | Receiver event `StreamingStarted` |
| 71-T10 | `test_ap1_receiver_flush` | During streaming, pyatv sends FLUSH | Receiver clears buffer, event `Flushed` |
| 71-T11 | `test_ap1_receiver_teardown` | pyatv sends TEARDOWN | Receiver stops playback, closes ports, event `Disconnected` |
| 71-T12 | `test_ap1_receiver_get_parameter_keepalive` | pyatv sends periodic GET_PARAMETER | Session stays alive, no timeout |
| 71-T13 | `test_ap1_receiver_cseq_tracking` | Multiple RTSP requests, verify CSeq echoed correctly | No RTSP protocol errors |
| 71-T14 | `test_ap1_receiver_session_management` | Verify Session header assigned and maintained | Session ID consistent across requests |

**Implementation notes:**
- The RTSP server for RAOP differs from AP2's HTTP-like server. RAOP uses standard RTSP with SDP bodies, while AP2 uses RTSP with binary plist bodies.
- Our sans-IO RTSP codec (`src/protocol/rtsp/`) handles both; the routing/dispatch layer determines AP1 vs AP2 based on the request structure.
- Reference: `src/receiver/rtsp_handler.rs` for RTSP handling, `src/receiver/announce_handler.rs` for ANNOUNCE parsing.

---

### 71.3 Authentication & Encryption Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Authentication and Encryption → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T15 | `test_ap1_receiver_no_encryption` | Configure `et=0`, pyatv connects without encryption | Connection succeeds, audio received unencrypted |
| 71-T16 | `test_ap1_receiver_rsa_aes_encryption` | Configure `et=1`, pyatv sends RSA-encrypted AES key | Receiver decrypts AES key, uses it for audio decryption |
| 71-T17 | `test_ap1_receiver_aes_key_from_sdp` | Verify receiver extracts `rsaaeskey` and `aesiv` from ANNOUNCE SDP | AES key and IV stored correctly in session state |
| 71-T18 | `test_ap1_receiver_encrypted_audio_decode` | Stream encrypted audio, verify decrypted output | Audio output matches expected sine wave |
| 71-T19 | `test_ap1_receiver_password_correct` | Set password on receiver, pyatv provides correct password | RTSP 200 OK, streaming works |
| 71-T20 | `test_ap1_receiver_password_wrong` | Set password, pyatv provides wrong password | RTSP 401/403, connection rejected |
| 71-T21 | `test_ap1_receiver_password_missing` | Set password, pyatv provides no password | RTSP 401, challenge sent |

**Implementation notes:**
- RSA encryption for RAOP: our receiver has an RSA keypair. The client encrypts a random AES-128 key with our public key and sends it in the ANNOUNCE SDP. We decrypt it with our private key and use it to decrypt RTP audio.
- Reference: `src/protocol/crypto/` for RSA and AES implementations.
- Password authentication uses HTTP Digest Auth or a custom RAOP auth challenge. pyatv supports password-protected receivers.

**Uncertainties:**
- pyatv's RSA key exchange may use a different padding scheme than expected. Test and compare with the RAOP spec.
- pyatv may not support all encryption modes (`et=0` vs `et=1` vs `et=3`). Verify pyatv's capabilities and skip unsupported modes.

---

### 71.4 Audio Streaming Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Audio RTP Handling → all items
- `airplay1-receiver-checklist.md` → Buffering and Audio Output → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T22 | `test_ap1_receiver_pcm_streaming` | pyatv streams 3s PCM WAV to our RAOP receiver | Audio file sink contains ~3s of correct sine wave |
| 71-T23 | `test_ap1_receiver_alac_streaming` | pyatv streams 3s ALAC to our RAOP receiver | Lossless audio verified |
| 71-T24 | `test_ap1_receiver_stereo` | pyatv streams stereo file | Both channels verified independently |
| 71-T25 | `test_ap1_receiver_long_stream` | pyatv streams 30s audio | No gaps, stable frequency, no drift |
| 71-T26 | `test_ap1_receiver_rtp_sequence_continuity` | During stream, verify receiver tracks sequence numbers | No sequence gaps in receiver stats |
| 71-T27 | `test_ap1_receiver_rtp_timestamp_tracking` | Verify timestamps increment correctly | Receiver timing stats show consistent intervals |
| 71-T28 | `test_ap1_receiver_352_frame_packets` | Verify receiver handles standard 352-frame RAOP packets | Packets decoded correctly |
| 71-T29 | `test_ap1_receiver_onset_latency` | Measure time from RECORD to first audio output | Latency < 3 seconds |
| 71-T30 | `test_ap1_receiver_jitter_buffer` | Verify jitter buffer fills before playback starts | Audio output smooth, no initial glitch |

**Implementation notes:**
- RAOP audio typically uses 352 samples per packet at 44100 Hz.
- AES-128-CBC decryption: each RTP payload is decrypted with the session AES key and IV. Only the first `N - (N % 16)` bytes are encrypted (AES block size), trailing bytes are unencrypted.
- Reference: `src/receiver/rtp_receiver.rs`, `src/audio/jitter.rs`.

---

### 71.5 Timing & Synchronization Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Timing Port Handling → all items
- `airplay1-receiver-checklist.md` → NTP-Like Synchronization → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T31 | `test_ap1_receiver_timing_responses` | pyatv sends timing requests (PT 0x52), verify receiver responds (PT 0x53) | Response contains originate/receive/transmit timestamps |
| 71-T32 | `test_ap1_receiver_sync_packet_processing` | pyatv sends sync packets (PT 0x54) on control port | Receiver processes sync info, clock offset computed |
| 71-T33 | `test_ap1_receiver_clock_stability` | Stream for 10s, verify timing offset stays stable | Clock offset drift < 10ms over 10s |
| 71-T34 | `test_ap1_receiver_timing_port_allocation` | Verify timing port from SETUP response is functional | UDP packets on timing port are answered |

**Implementation notes:**
- RAOP timing uses NTP-like packets (not actual NTP). Payload type 0x52 = timing request, 0x53 = timing response.
- Sync packets on the control port (PT 0x54) carry NTP timestamps and RTP timestamps, used to map between sender time and receiver time.
- Reference: `src/receiver/timing.rs`, `src/receiver/control_receiver.rs`.

---

### 71.6 Volume & Metadata Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Metadata, Volume, and Control → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T35 | `test_ap1_receiver_set_volume` | pyatv sends SET_PARAMETER volume during stream | Receiver event `VolumeChanged` with correct dB value |
| 71-T36 | `test_ap1_receiver_volume_range` | Set volume at extremes (-144 dB and 0 dB) | Both accepted without error |
| 71-T37 | `test_ap1_receiver_metadata_daap` | pyatv sends DAAP metadata (title/artist) | Receiver event `MetadataReceived` |
| 71-T38 | `test_ap1_receiver_artwork` | pyatv sends artwork via SET_PARAMETER | Receiver event with artwork data |
| 71-T39 | `test_ap1_receiver_progress` | pyatv sends progress (start/current/end) | Receiver tracks playback position |

**Uncertainties:**
- pyatv may not send metadata/artwork in RAOP mode. If not supported, document the gap and consider using a raw RTSP client to send the metadata manually.
- Volume in RAOP is sent as `SET_PARAMETER` with body `volume: -15.000000\r\n`. Our receiver must parse this text format.

---

### 71.7 Error Handling & Robustness Tests

**Checklist items covered:**
- `airplay1-receiver-checklist.md` → Error Handling and Robustness → all items
- `airplay1-receiver-checklist.md` → Session Management → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 71-T40 | `test_ap1_receiver_client_disconnect` | pyatv disconnects mid-stream | Receiver cleans up session, no crash |
| 71-T41 | `test_ap1_receiver_busy_rejection` | First client streaming, second tries to connect | Second client rejected or first preempted per policy |
| 71-T42 | `test_ap1_receiver_busy_flag` | During active session, verify `sf` TXT flag shows busy | TXT field `sf` updated |
| 71-T43 | `test_ap1_receiver_idle_timeout` | Connect, don't send RECORD, wait for timeout | Receiver disconnects idle client |
| 71-T44 | `test_ap1_receiver_malformed_announce` | Send ANNOUNCE with invalid SDP | Receiver responds with 400 Bad Request |
| 71-T45 | `test_ap1_receiver_missing_aes_key` | ANNOUNCE without rsaaeskey when encryption expected | Receiver handles gracefully |
| 71-T46 | `test_ap1_receiver_double_teardown` | Send TEARDOWN twice | No crash, second ignored or error returned |
| 71-T47 | `test_ap1_receiver_rapid_sessions` | 5 connect/stream/teardown cycles | All succeed, no resource leak |

---

## Checklist Cross-Reference

| Receiver Checklist Section | Items Covered | Tests |
|---|---|---|
| Service Discovery | RAOP advertisement, TXT records | 71-T1 through T5 |
| RTSP Server | OPTIONS through TEARDOWN | 71-T6 through T14 |
| Authentication & Encryption | RSA+AES, password, no-encryption | 71-T15 through T21 |
| Audio RTP Handling | Streaming, decryption, sequencing | 71-T22 through T30 |
| Timing & Sync | NTP timing, sync packets, clock | 71-T31 through T34 |
| Volume & Metadata | SET_PARAMETER, DAAP, artwork | 71-T35 through T39 |
| Error Handling | Disconnects, busy, malformed, timeouts | 71-T40 through T47 |

---

## Acceptance Criteria

- [ ] Our receiver accepts RAOP connections from pyatv
- [ ] SDP parsing extracts codec, encryption parameters correctly
- [ ] RSA+AES key exchange works with pyatv's implementation
- [ ] Audio decryption produces correct output for both PCM and ALAC
- [ ] Timing synchronization provides stable clock offset
- [ ] Volume and metadata changes are processed
- [ ] Error conditions handled without crash or resource leak
- [ ] All tests produce diagnostic artifacts on failure

---

## References

- `airplay1-receiver-checklist.md` — full AP1 receiver checklist
- `src/receiver/` — receiver implementation modules
- `src/receiver/announce_handler.rs` — SDP parsing
- `src/receiver/rtp_receiver.rs` — RTP packet reception
- `src/receiver/timing.rs` — NTP-like timing
- `src/protocol/raop/` — RAOP protocol types
- `tests/common/receiver_harness.rs` — Section 70 receiver harness
