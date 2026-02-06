# Section 70: AP2 Receiver vs pyatv — Tests

## Dependencies
- **Section 69**: pyatv Setup, Driver Scripts & Client Wrapper
- **Section 65**: Audio Verification & Analysis Framework
- **Section 64**: Subprocess Management Framework
- `airplay2-receiver-checklist.md` — checklist items verified by these tests

## Overview

This section defines integration tests for our AirPlay 2 receiver, validated by using pyatv as an external AirPlay 2 client. pyatv connects to our receiver, performs pairing, streams audio, and controls playback. Our receiver writes received audio to a file sink, which we then analyse to verify correctness.

This is the primary proof that our AP2 receiver works with a real client implementation — not just our own client code.

## Architecture

```
┌──────────────────────┐        ┌──────────────────────┐
│  pyatv (Python)      │  TCP   │  Our Receiver (Rust)  │
│  driver_ap2.py       │──────→ │  tests/common/        │
│  - pair-setup        │        │  receiver_harness.rs   │
│  - pair-verify       │  UDP   │  - RTSP server         │
│  - SETUP             │──────→ │  - RTP listener        │
│  - stream audio      │        │  - audio file sink     │
└──────────────────────┘        └──────────────────────┘
                                          │
                                   writes audio to
                                          │
                                 target/test-audio/
                                 received_audio.raw
                                          │
                              ┌───────────┘
                              ▼
                    Audio Verification
                    (Section 65)
```

---

## Tasks

### 70.1 Receiver Test Harness

**File:** `tests/common/receiver_harness.rs`

A Rust harness that starts our receiver as a tokio task within the test process (not a subprocess). This is faster and more controllable than spawning a separate process.

**Struct: `ReceiverHarness`**

Fields:
- `receiver_handle: JoinHandle<()>` — tokio task running the receiver
- `shutdown_tx: oneshot::Sender<()>` — signal to stop the receiver
- `port: u16` — RTSP listen port
- `events_rx: mpsc::Receiver<ReceiverEvent>` — receiver events (connected, streaming, etc.)
- `audio_sink_path: PathBuf` — where received audio is written
- `config: ReceiverTestConfig`

**Struct: `ReceiverTestConfig`**

Fields:
- `name: String` — service name (default: `"Test-Receiver-{random}"`)
- `port: u16` — RTSP port (from port allocator, or 0 for auto)
- `password: Option<String>` — AP2 pairing password
- `pin: String` — transient pairing PIN (default: `"3939"`)
- `audio_sink_path: PathBuf` — file path for received audio
- `enable_ap2: bool` — enable AirPlay 2 (default: true)
- `enable_ap1: bool` — enable AirPlay 1/RAOP (default: true)
- `advertise_mdns: bool` — whether to advertise via mDNS (default: false for most tests)
- `supported_codecs: Vec<AudioCodec>` — codecs to accept
- `log_level: String` — tracing log level

Methods:

**`async fn start(config: ReceiverTestConfig) -> Result<Self, ReceiverHarnessError>`**

Steps:
1. Reserve port if `config.port == 0`.
2. Create audio sink directory and path.
3. Build receiver configuration using our library types:
   - Use `SessionManagerConfig` from `src/receiver/session_manager.rs`
   - Set `idle_timeout`, `max_duration`, `preemption_policy`
   - Configure file-based audio sink (write decoded PCM to file)
4. Spawn receiver as a tokio task.
5. Wait for receiver to start listening (TCP health check on port, Section 64.5).
6. Return harness.

**`fn device_address(&self) -> SocketAddr`** — returns `127.0.0.1:{port}`.

**`fn device_config(&self) -> AirPlayDevice`** — construct device info for clients.

**`async fn stop(self) -> Result<ReceiverOutput, ReceiverHarnessError>`**

Steps:
1. Send shutdown signal via `shutdown_tx`.
2. Wait for receiver task to complete (with timeout).
3. Read audio file from `audio_sink_path`.
4. Collect events.
5. Return `ReceiverOutput` (reuse from Section 65 / existing code).

**`async fn wait_for_event(&mut self, event_type: EventType, timeout: Duration) -> Option<ReceiverEvent>`**

Poll events channel for a specific event type. Useful for asserting that pairing completed, streaming started, etc.

**Enum: `EventType`** — `Connected`, `PairingComplete`, `StreamingStarted`, `StreamingStopped`, `Disconnected`, `VolumeChanged`, `MetadataReceived`

---

### 70.2 Audio File Sink

**File:** `src/receiver/audio_sink.rs` (or `tests/common/audio_file_sink.rs`)

Our receiver needs an audio output backend that writes to a file instead of playing through speakers. This is analogous to shairport-sync's pipe backend and the Python receiver's `AIRPLAY_FILE_SINK=1` mode.

**Trait to implement:** `AudioOutput` from `src/audio/output.rs`

**Struct: `FileAudioSink`**

Fields:
- `path: PathBuf`
- `file: Option<File>`
- `bytes_written: u64`
- `format: AudioFormat`

Methods:
- `fn new(path: PathBuf, format: AudioFormat) -> Self`
- Implement `AudioOutput` trait:
  - `fn write(&mut self, samples: &[u8]) -> Result<(), AudioOutputError>` — append to file.
  - `fn flush(&mut self) -> Result<(), AudioOutputError>` — fsync.
  - `fn close(&mut self) -> Result<(), AudioOutputError>` — close file handle.
  - `fn state(&self) -> OutputState` — return `Playing`.

**Edge cases:**
- File creation failure (permissions, disk full) — return clear error.
- Concurrent writes (multiple audio streams) — not expected in tests, but guard with a mutex or document single-stream constraint.
- Large audio files — 30 seconds at CD quality = ~5 MB, manageable.

---

### 70.3 AP2 Service Advertisement Tests

**Checklist items covered:**
- `airplay2-receiver-checklist.md` → Service Discovery → `_airplay._tcp` Advertisement
- `airplay2-receiver-checklist.md` → Service Discovery → TXT Record

These tests verify our receiver advertises itself correctly via mDNS so that clients can discover it.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 70-T1 | `test_ap2_receiver_advertises_service` | Start receiver with `advertise_mdns: true`, verify `_airplay._tcp` service appears | pyatv `discover` action finds our receiver by name |
| 70-T2 | `test_ap2_receiver_txt_records` | Discover receiver, verify TXT record fields | `ch=2`, `cn` includes supported codecs, `sr=44100`, `et=4`, feature bits set |
| 70-T3 | `test_ap2_receiver_features_bitfield` | Verify feature flags in TXT record | Bits 9 (audio), 38 (pairing), 46 (HomeKit) set |
| 70-T4 | `test_ap2_receiver_info_endpoint` | pyatv `info` action against our receiver | Response contains required fields per `airplay2-receiver-checklist.md` section 2 |
| 70-T5 | `test_ap2_receiver_stops_advertisement` | Start receiver, verify advertised, stop receiver, verify de-advertised | Service disappears from mDNS within TTL |

**Implementation notes:**
- Tests 70-T1 through T3 require mDNS on loopback. Start Avahi (Section 66.4) or use our receiver's built-in mDNS advertisement.
- Reference: `src/discovery/advertiser.rs` for advertisement implementation.
- For T4, use pyatv's `info` action which sends `GET /info` and reports the response.

---

### 70.4 AP2 Pairing Tests

**Checklist items covered:**
- `airplay2-receiver-checklist.md` → HomeKit / HAP Pairing → pair-setup, pair-verify
- `airplay2-receiver-checklist.md` → HomeKit / HAP Pairing → Session Encryption

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 70-T6 | `test_ap2_receiver_transient_pairing` | pyatv pairs with our receiver using PIN 3939 | `PyAtvResult.success == true`, receiver event `PairingComplete` |
| 70-T7 | `test_ap2_receiver_wrong_pin` | pyatv attempts pairing with wrong PIN | `PyAtvResult.success == false`, error mentions authentication |
| 70-T8 | `test_ap2_receiver_pair_verify` | pyatv completes pair-setup then pair-verify | Encrypted channel established, subsequent commands work |
| 70-T9 | `test_ap2_receiver_persistent_pairing` | pyatv pairs, disconnects, reconnects with stored credentials | Second connection uses pair-verify only |
| 70-T10 | `test_ap2_receiver_encrypted_control` | After pairing, verify control commands are encrypted | Receiver processes encrypted SETUP/RECORD without error |
| 70-T11 | `test_ap2_receiver_rejects_unauthenticated` | pyatv skips pairing, sends SETUP directly | Receiver rejects with 403 or connection closed |

**Edge cases:**
- pyatv's pairing implementation may use different SRP parameters than Apple devices. Our receiver must handle both.
- Persistent pairing requires our receiver to store pairing records. Configure a temp directory for the credential store.
- If pyatv doesn't support persistent pairing, test 70-T9 should be skipped.

---

### 70.5 AP2 SETUP & Streaming Tests

**Checklist items covered:**
- `airplay2-receiver-checklist.md` → AirPlay 2 SETUP Phases → Phase 1, Phase 2
- `airplay2-receiver-checklist.md` → RTP, Encryption, and Timing → RTP Data Channel

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 70-T12 | `test_ap2_receiver_pcm_streaming` | pyatv streams 3s WAV (440 Hz PCM) to our receiver | Audio file sink contains ~3s of 440 Hz sine wave |
| 70-T13 | `test_ap2_receiver_alac_streaming` | pyatv streams 3s ALAC to our receiver | Audio verified, lossless check passes |
| 70-T14 | `test_ap2_receiver_stereo` | pyatv streams stereo file (440 Hz L, 880 Hz R) | Both channels verified independently |
| 70-T15 | `test_ap2_receiver_long_stream` | pyatv streams 30s audio | No gaps, frequency stable, no drift |
| 70-T16 | `test_ap2_receiver_encrypted_audio` | Verify RTP audio is decrypted correctly | Audio output matches expected sine wave |
| 70-T17 | `test_ap2_receiver_setup_phases` | Verify receiver correctly processes phase 1 (timing/event) and phase 2 (audio) | Receiver events show both phases completed |
| 70-T18 | `test_ap2_receiver_audio_format_negotiation` | Verify receiver accepts the format pyatv requests | No format mismatch errors |

**Implementation notes:**
- The audio verification uses `RawAudio::from_file(audio_sink_path, RawAudioFormat::CD_QUALITY)` → `SineWaveCheck::verify()`.
- pyatv may choose its own codec. Check `PyAtvResult.streaming_stats` for the codec used.
- If pyatv only supports one codec, adjust tests accordingly and document the gap.

---

### 70.6 AP2 Control & Metadata Tests

**Checklist items covered:**
- `airplay2-receiver-checklist.md` → Metadata, Volume, and Control → all items
- `airplay2-receiver-checklist.md` → Buffering, Latency, and Playback → FLUSH, pause/resume

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 70-T19 | `test_ap2_receiver_volume_change` | pyatv changes volume during streaming | Receiver event `VolumeChanged` with correct dB value |
| 70-T20 | `test_ap2_receiver_pause_resume` | pyatv pauses, then resumes playback | Gap in audio during pause, audio resumes after |
| 70-T21 | `test_ap2_receiver_teardown` | pyatv sends TEARDOWN | Receiver event `Disconnected`, ports cleaned up |
| 70-T22 | `test_ap2_receiver_feedback_heartbeat` | pyatv sends /feedback periodically during stream | No session timeout, stream continues |
| 70-T23 | `test_ap2_receiver_metadata` | pyatv sends track metadata | Receiver event `MetadataReceived` with title/artist |

**Uncertainties:**
- pyatv may not support all control commands (e.g., metadata sending). Document which commands pyatv supports and which tests must be skipped.
- Volume range: pyatv may send volume as 0-100 float; our receiver expects -144 to 0 dB. Verify the conversion.

---

### 70.7 Error Handling & Robustness Tests

**Checklist items covered:**
- `airplay2-receiver-checklist.md` → Error Handling and Robustness → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 70-T24 | `test_ap2_receiver_client_disconnect` | pyatv disconnects mid-stream (kill driver) | Receiver cleans up session, no crash, ready for new connection |
| 70-T25 | `test_ap2_receiver_second_client` | First pyatv connects, second pyatv tries to connect | Behaviour depends on `PreemptionPolicy` — verify per policy |
| 70-T26 | `test_ap2_receiver_rapid_sessions` | 5 pyatv connect/stream/disconnect cycles | All succeed, no resource leak |
| 70-T27 | `test_ap2_receiver_idle_timeout` | Connect, don't stream, wait for idle timeout | Receiver disconnects client after timeout |
| 70-T28 | `test_ap2_receiver_malformed_data` | Send garbage bytes to receiver port (not via pyatv, via raw TCP) | Receiver rejects gracefully, no crash |
| 70-T29 | `test_ap2_receiver_graceful_shutdown` | Stop receiver while pyatv is streaming | pyatv detects disconnect, receiver exits cleanly |

---

## Checklist Cross-Reference

| Receiver Checklist Section | Items Covered | Tests |
|---|---|---|
| 1. Service Discovery | Advertisement, TXT records | 70-T1 through T5 |
| 2. RTSP/HTTP Server | GET /info, POST /pair-*, SETUP | 70-T4, T6 through T11, T17 |
| 3. HomeKit / HAP Pairing | pair-setup, pair-verify, encryption | 70-T6 through T11 |
| 4. AirPlay 2 SETUP Phases | Phase 1 + 2 | 70-T17, T18 |
| 5. RTP, Encryption, Timing | RTP reception, decryption | 70-T12 through T16 |
| 6. Buffering, Latency, Playback | FLUSH, pause/resume | 70-T20 |
| 7. Metadata, Volume, Control | SET_PARAMETER, /command, /feedback | 70-T19 through T23 |
| 9. Error Handling | Malformed data, timeouts, cleanup | 70-T24 through T29 |

---

## Acceptance Criteria

- [ ] Our receiver accepts connections from pyatv and streams audio successfully
- [ ] Pairing (transient and persistent) works correctly
- [ ] Audio output is verified as correct sine wave at expected frequency
- [ ] Volume and metadata commands are processed
- [ ] Error conditions are handled gracefully
- [ ] No resource leaks across multiple test runs
- [ ] Tests produce diagnostic output on failure (audio dumps, receiver logs)

---

## References

- `airplay2-receiver-checklist.md` — full receiver checklist
- `src/receiver/session_manager.rs` — `SessionManager`, `SessionManagerConfig`
- `src/receiver/session.rs` — `SessionState`, `StreamParameters`
- `src/audio/output.rs` — `AudioOutput` trait
- `tests/common/pyatv.rs` — Section 69 pyatv wrapper
- `tests/common/receiver_harness.rs` — this section's receiver harness
