# Section 74: AP1 Loopback Tests

## Dependencies
- **Section 72**: Loopback Test Infrastructure
- **Section 65**: Audio Verification & Analysis Framework
- `airplay1-checklist.md` — client-side items
- `airplay1-receiver-checklist.md` — receiver-side items

## Overview

These tests exercise our AirPlay 1 (RAOP) client against our own universal receiver in loopback mode. They form the AP1 regression test suite, complementing the AP2 loopback tests in Section 73. The RAOP protocol has significant differences from AP2 (SDP negotiation, RSA+AES encryption, NTP timing), so these tests cover a distinct code path through both client and receiver.

---

## Tasks

### 74.1 Full Session Lifecycle Tests

Verify the complete RAOP session: OPTIONS → ANNOUNCE → SETUP → RECORD → stream → TEARDOWN.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T1 | `test_ap1_loopback_full_session` | Complete RAOP session with streaming | Events: Connected, Announced, Setup, StreamingStarted, Disconnected |
| 74-T2 | `test_ap1_loopback_connect_disconnect` | Connect and disconnect without streaming | Clean teardown, no errors |
| 74-T3 | `test_ap1_loopback_multiple_sessions` | Stream, disconnect, reconnect, stream again | Both streams verified |
| 74-T4 | `test_ap1_loopback_rapid_sessions` | 10 connect/stream/disconnect cycles | All succeed, no resource leak |
| 74-T5 | `test_ap1_loopback_long_session_60s` | Stream 60s continuous audio | No drift, no gaps |

**Implementation notes:**
- Use `LoopbackConfig` with `protocol: LoopbackProtocol::Raop`.
- Client config: `PreferredProtocol::ForceRaop`.
- RAOP session lifecycle differs from AP2 — our client sends ANNOUNCE (with SDP) before SETUP, there are no binary plist bodies.

---

### 74.2 Authentication & Encryption Mode Tests

RAOP supports multiple encryption modes, unlike AP2 which only uses ChaCha20-Poly1305.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T6 | `test_ap1_loopback_no_encryption` | Both sides use `et=0`, no encryption | Audio streams unencrypted, verified |
| 74-T7 | `test_ap1_loopback_rsa_aes_encryption` | Client encrypts AES key with RSA, receiver decrypts | Audio decrypted correctly, sine wave verified |
| 74-T8 | `test_ap1_loopback_aes_key_roundtrip` | Verify the AES key encrypted by client matches key decrypted by receiver | Compare keys in instrumented test |
| 74-T9 | `test_ap1_loopback_password_correct` | Receiver requires password, client provides correct one | Connection succeeds |
| 74-T10 | `test_ap1_loopback_password_wrong` | Receiver requires password, client provides wrong one | `AirPlayError::AuthenticationFailed` |
| 74-T11 | `test_ap1_loopback_password_none_required` | Receiver requires password, client provides none | RTSP 401 response |
| 74-T12 | `test_ap1_loopback_encryption_mode_negotiation` | Receiver advertises `et=0,1`, client selects preferred | Client uses RSA if available |

**Implementation notes:**
- For no-encryption tests, configure receiver with `encryption_types: vec![RaopEncryption::None]`.
- For RSA+AES tests, the receiver needs an RSA keypair. Use a test keypair generated at startup (not hardcoded).
- AES key roundtrip test (T8): instrument both client and receiver to log the AES key. Compare in test.
- Password tests: set `password` on `ReceiverTestConfig`, set `pin` on `AirPlayConfig`.
- Reference: `src/protocol/crypto/` for RSA, AES implementations.

---

### 74.3 SDP Negotiation Tests

RAOP uses SDP in the ANNOUNCE request to negotiate audio format and encryption parameters.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T13 | `test_ap1_loopback_sdp_pcm` | Client announces PCM via SDP (`L16/44100/2`) | Receiver parses SDP, sets up PCM pipeline |
| 74-T14 | `test_ap1_loopback_sdp_alac` | Client announces ALAC via SDP (`AppleLossless`) | Receiver parses ALAC fmtp parameters correctly |
| 74-T15 | `test_ap1_loopback_sdp_with_encryption` | SDP includes `rsaaeskey` and `aesiv` | Receiver extracts and decrypts AES key |
| 74-T16 | `test_ap1_loopback_sdp_min_latency` | SDP includes `min-latency` attribute | Receiver honors minimum latency setting |
| 74-T17 | `test_ap1_loopback_sdp_fmtp_alac_params` | Verify ALAC fmtp line includes frame length, bit depth, channels | Receiver parses all ALAC parameters |

**Implementation notes:**
- SDP generation is in our RAOP client (`src/streaming/raop_streamer.rs` or `src/client/session.rs`).
- SDP parsing is in our receiver (`src/receiver/announce_handler.rs`).
- These tests verify that our SDP encoding and decoding are mutually compatible.
- fmtp line for ALAC: `a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100`

---

### 74.4 Codec Matrix Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T18 | `test_ap1_loopback_pcm_44100` | PCM at 44100 Hz | Audio verified, bit-exact |
| 74-T19 | `test_ap1_loopback_alac_44100` | ALAC at 44100 Hz | Lossless audio verified |
| 74-T20 | `test_ap1_loopback_pcm_48000` | PCM at 48000 Hz (if supported) | Audio verified |
| 74-T21 | `test_ap1_loopback_codec_matrix` | Parametric: codec × sample rate × encryption mode | Use `TestMatrix` from Section 72.4 |

Extended matrix dimensions for AP1:
- Codecs: PCM, ALAC (AAC if implemented)
- Sample rates: 44100 (48000 if supported)
- Encryption: None, RSA+AES-128

---

### 74.5 Transport & Port Negotiation Tests

RAOP's SETUP response includes `server_port`, `control_port`, and `timing_port`.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T22 | `test_ap1_loopback_setup_port_negotiation` | Client sends SETUP, receiver allocates ports, client uses them | Ports are valid and functional |
| 74-T23 | `test_ap1_loopback_control_port_functional` | After SETUP, sync packets flow on control port | Receiver processes sync data |
| 74-T24 | `test_ap1_loopback_timing_port_functional` | After SETUP, timing exchange works on timing port | Timing requests answered |
| 74-T25 | `test_ap1_loopback_audio_port_functional` | RTP audio packets received on server_port | Audio decoded and output |

---

### 74.6 Timing & Synchronization Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T26 | `test_ap1_loopback_ntp_timing_exchange` | Client sends timing requests, receiver responds | Three NTP timestamps in response |
| 74-T27 | `test_ap1_loopback_sync_packet_flow` | Client sends sync packets during streaming | Receiver computes clock offset |
| 74-T28 | `test_ap1_loopback_clock_stability_30s` | Stream for 30s, verify clock offset stays stable | Offset drift < 10ms |
| 74-T29 | `test_ap1_loopback_latency_measurement` | Measure onset latency | Latency within expected AP1 range (1-3 seconds) |
| 74-T30 | `test_ap1_loopback_sync_after_flush` | FLUSH, then resume — verify sync re-established | Audio output aligned after flush |

---

### 74.7 RTP Packet Handling Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T31 | `test_ap1_loopback_rtp_sequence_continuous` | Stream audio, verify no sequence gaps at receiver | Receiver sequence tracker reports 0 gaps |
| 74-T32 | `test_ap1_loopback_rtp_timestamp_increment` | Verify timestamp increases by 352 per packet | Receiver timing logs show consistent 352-sample increments |
| 74-T33 | `test_ap1_loopback_rtp_352_frame_packets` | Verify packet size matches RAOP standard | Receiver receives expected packet sizes |
| 74-T34 | `test_ap1_loopback_rtp_aes_decryption` | Encrypted RTP payloads decrypted correctly | Audio output matches expected sine wave |
| 74-T35 | `test_ap1_loopback_rtp_partial_encryption` | Verify only first `N - (N%16)` bytes encrypted (AES block alignment) | Trailing bytes preserved, audio correct |

**Implementation notes:**
- RAOP encrypts only full AES blocks (16 bytes). If the payload is 353 bytes, only the first 352 bytes (22 blocks × 16) are encrypted. The last byte is unencrypted. Verify our implementation handles this correctly.
- Reference: `src/receiver/rtp_receiver.rs`, `src/protocol/raop/`.

---

### 74.8 Volume, Metadata & Control Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T36 | `test_ap1_loopback_set_volume` | Set volume via SET_PARAMETER during stream | Receiver processes volume change |
| 74-T37 | `test_ap1_loopback_volume_text_format` | Verify volume sent as `volume: -15.000000\r\n` | Receiver parses text format correctly |
| 74-T38 | `test_ap1_loopback_flush_and_resume` | FLUSH then stream new audio | New audio verified, old audio dropped |
| 74-T39 | `test_ap1_loopback_metadata_daap` | Send DAAP metadata (title/artist) | Receiver receives and stores metadata |
| 74-T40 | `test_ap1_loopback_get_parameter_keepalive` | Send GET_PARAMETER during idle session | Session stays alive |

---

### 74.9 Error Handling & Edge Cases

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T41 | `test_ap1_loopback_receiver_killed` | Stop receiver mid-stream | Client detects disconnect |
| 74-T42 | `test_ap1_loopback_client_killed` | Drop client mid-stream | Receiver cleans up session |
| 74-T43 | `test_ap1_loopback_double_announce` | Client sends ANNOUNCE twice | Receiver handles gracefully |
| 74-T44 | `test_ap1_loopback_setup_without_announce` | Client sends SETUP before ANNOUNCE | Receiver rejects with error |
| 74-T45 | `test_ap1_loopback_record_without_setup` | Client sends RECORD before SETUP | Receiver rejects with error |
| 74-T46 | `test_ap1_loopback_busy_rejection` | Two clients try to stream to same receiver | One rejected per `PreemptionPolicy` |
| 74-T47 | `test_ap1_loopback_100_sessions` | 100 sequential sessions | All succeed, no resource leak |
| 74-T48 | `test_ap1_loopback_empty_audio` | Send RECORD but no audio packets | Receiver handles silence gracefully |

---

### 74.10 Audio Quality Deep Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 74-T49 | `test_ap1_loopback_stereo_independence` | 440 Hz left, 880 Hz right | Channels verified independently |
| 74-T50 | `test_ap1_loopback_bit_exact_pcm` | PCM round-trip bit-exact comparison | `CompareResult.bit_exact == true` |
| 74-T51 | `test_ap1_loopback_bit_exact_alac` | ALAC round-trip bit-exact | `CompareResult.bit_exact == true` |
| 74-T52 | `test_ap1_loopback_frequency_sweep` | Frequencies: 100, 440, 1000, 4000, 10000 Hz | All detected correctly |
| 74-T53 | `test_ap1_loopback_max_amplitude` | Full-scale ±32767 sine wave | No clipping or distortion |

---

## Checklist Cross-Reference

| AP1 Client Checklist | Tests |
|---|---|
| Audio Format Support | 74-T18 through T21 |
| Latency and Jitter | 74-T29 |
| Service Discovery | (covered in Section 71) |
| RSA + AES-128 Flow | 74-T7, T8, T15, T34, T35 |
| Password Authentication | 74-T9 through T11 |
| RTSP Session | 74-T1, T13 through T17, T22 |
| RTP Audio Stream | 74-T31 through T35 |
| Timing / Sync | 74-T26 through T30 |
| Error Handling | 74-T41 through T48 |

| AP1 Receiver Checklist | Tests |
|---|---|
| RAOP Service Advertisement | (covered in Section 71) |
| RTSP Server Methods | 74-T1, T22 through T25 |
| ANNOUNCE Handling | 74-T13 through T17 |
| RSA+AES Decryption | 74-T7, T34, T35 |
| Password Protection | 74-T9 through T11 |
| RTP Receiver | 74-T31 through T35 |
| Timing | 74-T26 through T30 |
| Jitter Buffer | 74-T30 |
| Volume & Metadata | 74-T36 through T40 |
| Session Management | 74-T46 |

---

## Acceptance Criteria

- [ ] Full RAOP session lifecycle works in loopback
- [ ] Both encryption modes (none and RSA+AES) work correctly
- [ ] SDP negotiation correctly passes codec and encryption parameters
- [ ] All codec × encryption combinations produce verified audio
- [ ] Timing synchronization stable over 30+ seconds
- [ ] Port negotiation works correctly
- [ ] 100-session stress test passes without resource leaks
- [ ] Bit-exact comparison passes for lossless codecs

---

## References

- `airplay1-checklist.md` — AP1 client checklist
- `airplay1-receiver-checklist.md` — AP1 receiver checklist
- `tests/common/loopback.rs` — Section 72 loopback infrastructure
- `src/client/session.rs` — `RaopSessionImpl`
- `src/streaming/raop_streamer.rs` — RAOP streamer
- `src/receiver/announce_handler.rs` — SDP parsing
- `src/receiver/rtp_receiver.rs` — RTP reception
