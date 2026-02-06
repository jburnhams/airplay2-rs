# Section 67: AP1 Client vs shairport-sync — Tests

## Dependencies
- **Section 66**: shairport-sync Build, Configuration & Subprocess Wrapper
- **Section 65**: Audio Verification & Analysis Framework
- **Section 64**: Subprocess Management Framework
- `airplay1-checklist.md` — checklist items verified by these tests

## Overview

This section defines integration tests that exercise our AirPlay 1 (RAOP) client against shairport-sync running as a reference receiver. These tests are the primary proof that our RAOP implementation is compatible with a real-world AirPlay 1 receiver. Every test follows the pattern: start shairport-sync, connect our client, perform an action, verify the outcome.

## Objectives

- Verify RAOP service discovery and TXT record parsing
- Verify RSA authentication and AES-128 key exchange
- Verify RTSP session lifecycle (OPTIONS → ANNOUNCE → SETUP → RECORD → TEARDOWN)
- Verify audio streaming for all supported codecs (PCM, ALAC)
- Verify timing synchronization
- Verify volume control and metadata delivery
- Verify error handling (wrong password, connection loss, malformed responses)

---

## Shared Test Setup

Every test in this file follows this structure:

1. Create `ShairportConfig` with AP1 settings (`airplay2_enabled: false`).
2. Start `ShairportSync::start(config)`.
3. Construct `AirPlayDevice` via `shairport.device_config()`.
4. Configure `AirPlayClient` with `PreferredProtocol::ForceRaop` to ensure AP1 path.
5. Run test scenario.
6. Stop shairport-sync and verify audio output.

**Shared helper function proposed:**

```
async fn with_shairport_ap1<F, Fut>(config_overrides: ShairportConfig, test: F)
where
    F: FnOnce(ShairportSync, AirPlayDevice) -> Fut,
    Fut: Future<Output = Result<(), Box<dyn Error>>>,
```

This helper handles setup/teardown boilerplate and ensures cleanup on panic.

---

## Tasks

### 67.1 Connection & Discovery Tests

These tests verify the client can discover and connect to shairport-sync.

**Checklist items covered:**
- `airplay1-checklist.md` → Service Discovery → all items
- `airplay1-checklist.md` → TXT Record Parsing → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T1 | `test_ap1_connect_to_shairport` | Basic connection: client connects, sends OPTIONS, gets 200 OK | `client.is_connected() == true` |
| 67-T2 | `test_ap1_disconnect_clean` | Connect, then disconnect. Verify shairport-sync logs session teardown | No error on disconnect, shairport logs "TEARDOWN" |
| 67-T3 | `test_ap1_discovery_finds_shairport` | Start shairport-sync with Avahi (Section 66.4), run `client.scan()`, verify device found | `scan()` returns device with matching name |
| 67-T4 | `test_ap1_txt_record_parsing` | Discover shairport-sync, verify parsed `RaopCapabilities` match expected values | `caps.codecs` contains Pcm and Alac, `caps.sample_rate == 44100` |
| 67-T5 | `test_ap1_reconnect_after_disconnect` | Connect, disconnect, reconnect. Verify second connection succeeds | Second `client.connect()` returns Ok |
| 67-T6 | `test_ap1_connect_timeout` | Connect to non-existent address. Verify timeout error | `AirPlayError::ConnectionTimeout` returned |

**Edge cases:**
- shairport-sync may take 2-3 seconds to register with Avahi after startup. Discovery test must retry or wait.
- If Avahi is not available, skip discovery tests with `#[ignore]` and a clear message.

---

### 67.2 Authentication & Encryption Tests

**Checklist items covered:**
- `airplay1-checklist.md` → Authentication and Encryption → RSA + AES-128 Flow
- `airplay1-checklist.md` → Authentication and Encryption → Password Authentication

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T7 | `test_ap1_rsa_aes_encryption` | Connect with encryption enabled (`et=1`). Stream audio. Verify audio received | Audio data present and non-zero in pipe output |
| 67-T8 | `test_ap1_no_encryption` | Configure shairport-sync with `et=0`. Connect without encryption. Stream | Audio received, no encryption negotiated |
| 67-T9 | `test_ap1_password_correct` | Set shairport-sync password. Client connects with matching password | Connection succeeds, streaming works |
| 67-T10 | `test_ap1_password_wrong` | Set shairport-sync password. Client attempts wrong password | `AirPlayError::AuthenticationFailed` or RTSP 401/403 |
| 67-T11 | `test_ap1_password_none_when_required` | Set shairport-sync password. Client connects without password | Connection rejected |
| 67-T12 | `test_ap1_aes_key_exchange_validity` | Connect with RSA encryption, verify ANNOUNCE SDP contains `rsaaeskey` and `aesiv` fields | Check shairport-sync logs for successful key decryption |

**Implementation notes:**
- Password testing requires setting `password` in `ShairportConfig` and `pw=true` in the TXT record.
- RSA key exchange verification: our client generates random AES key + IV, RSA-encrypts with the receiver's public key, and includes them in the ANNOUNCE SDP body. shairport-sync decrypts them. If decryption fails, shairport-sync logs an error and rejects the session. Check logs for absence of decryption errors.
- Reference: `src/protocol/crypto/` for RSA implementation, `src/client/session.rs` for `RaopSessionImpl`.

---

### 67.3 RTSP Session Lifecycle Tests

**Checklist items covered:**
- `airplay1-checklist.md` → RTSP Session → all items
- `airplay1-checklist.md` → SETUP / RECORD / FLUSH / TEARDOWN → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T13 | `test_ap1_options_method` | Send OPTIONS, verify response lists supported methods | Response contains `ANNOUNCE, SETUP, RECORD, FLUSH, TEARDOWN` |
| 67-T14 | `test_ap1_announce_sdp_format` | Connect and verify ANNOUNCE is sent with valid SDP body | Check shairport-sync logs for parsed SDP fields |
| 67-T15 | `test_ap1_setup_transport_negotiation` | After ANNOUNCE, send SETUP and verify transport header in response | Response contains `server_port`, `control_port`, `timing_port` |
| 67-T16 | `test_ap1_record_starts_playback` | Full flow through RECORD, verify shairport-sync begins receiving RTP | Logs show "play" or RTP reception |
| 67-T17 | `test_ap1_flush_clears_buffer` | Stream audio, send FLUSH, stream new audio. Verify no overlap | Audio output does not contain old signal after flush point |
| 67-T18 | `test_ap1_teardown_closes_session` | Full session then TEARDOWN. Verify ports closed | Subsequent connection attempt on same shairport-sync works |
| 67-T19 | `test_ap1_cseq_tracking` | Verify CSeq increments correctly across multiple RTSP requests | No RTSP errors in shairport-sync logs |
| 67-T20 | `test_ap1_session_header_persistence` | Verify Session header is included after SETUP | Shairport-sync maintains single session |

**Implementation notes:**
- Most of these tests operate at the `AirPlayClient` level, which internally drives the RTSP session via `RaopSessionImpl` (`src/client/session.rs`).
- For tests that need to verify specific RTSP messages (e.g., 67-T13, 67-T14), configure the client with `debug_protocol: true` in `AirPlayConfig` and check the tracing output.
- FLUSH testing (67-T17): send a 440 Hz sine wave, FLUSH, then send 880 Hz. Verify the pipe output transitions from 440 Hz to 880 Hz without mixing.

---

### 67.4 Audio Streaming Tests

**Checklist items covered:**
- `airplay1-checklist.md` → Audio Format Support → PCM, ALAC
- `airplay1-checklist.md` → RTP Audio Stream → all items
- `airplay1-checklist.md` → Buffering and Playback → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T21 | `test_ap1_pcm_streaming_44100` | Stream 3s of 440 Hz PCM at 44100 Hz | Frequency match, amplitude correct, duration ~3s |
| 67-T22 | `test_ap1_alac_streaming_44100` | Stream 3s of 440 Hz ALAC at 44100 Hz | Frequency match, ALAC decode lossless |
| 67-T23 | `test_ap1_stereo_independence` | Stream 440 Hz left, 880 Hz right | Both channels verified independently |
| 67-T24 | `test_ap1_long_stream_no_drift` | Stream 30s continuous audio | No gaps, frequency stable throughout |
| 67-T25 | `test_ap1_short_stream` | Stream 500ms of audio | Audio received, correctly decoded |
| 67-T26 | `test_ap1_multiple_streams` | Stream, stop, stream again on same session | Both streams verified, no corruption |
| 67-T27 | `test_ap1_rtp_sequence_numbers` | Stream audio, verify RTP sequence numbers are sequential in shairport-sync logs | No gaps reported |
| 67-T28 | `test_ap1_rtp_timestamp_continuity` | Verify RTP timestamps increment by expected frame count per packet | shairport-sync timing logs show consistent intervals |
| 67-T29 | `test_ap1_352_frame_packets` | Verify our client sends 352-frame packets (standard RAOP packet size) | shairport-sync receives expected packet sizes |

**Implementation notes:**
- Use `AirPlayConfig::builder().audio_codec(AudioCodec::Pcm)` or `AudioCodec::Alac` to select codec.
- Use `PreferredProtocol::ForceRaop` to ensure AP1 path.
- Use `TestSineSource::new(440.0, 3.0)` from `tests/common/python_receiver.rs` for audio generation.
- Verify audio via `ShairportOutput::to_raw_audio()` → `SineWaveCheck::verify()` (Section 65).
- Long-stream test (67-T24): 30 seconds is enough to detect clock drift issues. Check that the estimated frequency remains stable across 1-second windows.

**Audio capture from shairport-sync:** with pipe backend, shairport-sync writes decoded 16-bit signed LE PCM to the pipe. This is the final decoded output regardless of input codec. Format is determined by the `pipe` section in the config file.

---

### 67.5 Timing & Sync Tests

**Checklist items covered:**
- `airplay1-checklist.md` → Control and Timing Packets → all items
- `airplay1-checklist.md` → NTP-Based Time Sync → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T30 | `test_ap1_timing_port_active` | After SETUP, verify timing port responds to timing requests | shairport-sync logs show timing exchange |
| 67-T31 | `test_ap1_control_port_sync` | During streaming, verify sync packets sent on control port | shairport-sync logs show sync reception |
| 67-T32 | `test_ap1_latency_within_bounds` | Measure onset latency of received audio | Latency < 3 seconds (AP1 default ~2s buffer) |
| 67-T33 | `test_ap1_clock_sync_stability` | Stream for 10s, check timing offset doesn't grow | shairport-sync sync reports show stable offset |

**Implementation notes:**
- Timing and sync verification primarily comes from shairport-sync's diagnostic output. Configure `log_verbosity = 3` for detailed timing logs.
- Parse shairport-sync log lines for timing offset values to verify stability.
- Onset latency: use `measure_onset_latency()` from Section 65 on the captured audio.

---

### 67.6 Volume & Metadata Tests

**Checklist items covered:**
- `airplay1-checklist.md` → Optional AirPlay 1 Extras → Volume, Metadata

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T34 | `test_ap1_set_volume_during_playback` | Stream audio, set volume to -15 dB mid-stream | shairport-sync logs show volume change |
| 67-T35 | `test_ap1_volume_range` | Set volume to min (-144 dB) and max (0 dB) | Both accepted without error |
| 67-T36 | `test_ap1_mute_unmute` | Mute during playback, then unmute | Audio resumes after unmute |
| 67-T37 | `test_ap1_metadata_track_info` | Set track metadata (title, artist, album) during stream | Metadata pipe output contains expected DAAP data |
| 67-T38 | `test_ap1_metadata_artwork` | Send JPEG artwork during stream | Metadata pipe contains artwork data |

**Implementation notes:**
- Volume: use `client.set_volume()` which sends `SET_PARAMETER volume:` via RTSP.
- Metadata: use `client.set_metadata(TrackMetadata { title: "Test", artist: "Artist", .. })`.
- Metadata pipe format: shairport-sync writes metadata in a specific binary format. Parse the first few bytes to identify the metadata type, then extract the payload.
- Artwork test may require the metadata pipe to be configured and a reader attached.

---

### 67.7 Error Handling & Edge Cases

**Checklist items covered:**
- `airplay1-checklist.md` → Error Handling and Robustness → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 67-T39 | `test_ap1_server_disconnect_during_stream` | Kill shairport-sync mid-stream, verify client detects loss | `AirPlayError::Disconnected` or connection lost event |
| 67-T40 | `test_ap1_stream_after_flush` | FLUSH then immediately stream new audio | New audio received without error |
| 67-T41 | `test_ap1_double_teardown` | Send TEARDOWN twice | No crash, second returns error or is ignored |
| 67-T42 | `test_ap1_connect_while_busy` | Two clients connect to same shairport-sync simultaneously | One succeeds, one fails or preempts depending on shairport-sync config |
| 67-T43 | `test_ap1_keepalive_maintains_session` | Connect, wait 60s without streaming, verify session still alive | `GET_PARAMETER` keep-alive prevents timeout |
| 67-T44 | `test_ap1_rapid_connect_disconnect` | Connect/disconnect 10 times in quick succession | All succeed, no resource leaks |
| 67-T45 | `test_ap1_oversized_packet_handling` | Send oversized RTP packet to shairport-sync | Session not disrupted, error logged |

---

## Checklist Cross-Reference

| Checklist Section | Items Covered | Tests |
|---|---|---|
| Audio Format Support | PCM, ALAC | 67-T21, T22 |
| Latency and Jitter | 2s latency, buffer | 67-T32 |
| Service Discovery | mDNS, TXT records | 67-T3, T4 |
| RSA + AES-128 Flow | Key exchange, encryption | 67-T7, T8, T12 |
| Password Authentication | Correct/wrong password | 67-T9, T10, T11 |
| RTSP Session | OPTIONS through TEARDOWN | 67-T13 through T20 |
| RTP Audio Stream | Sequencing, timestamps, packets | 67-T27, T28, T29 |
| Timing / Sync | NTP timing, control sync | 67-T30 through T33 |
| Volume / Metadata | SET_PARAMETER, DAAP | 67-T34 through T38 |
| Error Handling | Disconnect, busy, keepalive | 67-T39 through T45 |

---

## Acceptance Criteria

- [ ] All PCM and ALAC streaming tests produce verified sine wave output
- [ ] RSA key exchange works correctly against shairport-sync
- [ ] Password authentication accepts correct and rejects wrong passwords
- [ ] RTSP session lifecycle completes without protocol errors
- [ ] Volume changes are reflected in shairport-sync logs
- [ ] Long-stream tests show no timing drift
- [ ] Error handling tests don't crash or leak resources

---

## References

- `airplay1-checklist.md` — full checklist of AP1 client features
- `src/client/session.rs` — `RaopSessionImpl`
- `src/client/protocol.rs` — `PreferredProtocol::ForceRaop`
- `src/streaming/raop_streamer.rs` — `RaopStreamer`
- `src/protocol/raop/` — RAOP protocol implementation
- `src/types/raop.rs` — `RaopCapabilities`, `RaopCodec`, `RaopEncryption`
