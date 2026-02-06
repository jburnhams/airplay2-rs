# Section 73: AP2 Loopback Tests

## Dependencies
- **Section 72**: Loopback Test Infrastructure
- **Section 65**: Audio Verification & Analysis Framework
- `airplay2-checklist.md` — client-side items
- `airplay2-receiver-checklist.md` — receiver-side items

## Overview

These tests exercise our AirPlay 2 client against our own AirPlay 2 receiver in the same process. They serve as regression tests that lock in the behavior verified by Phase 1 third-party tests (Sections 67–71). When a Phase 1 test passes, the corresponding loopback test should also pass. If a future code change breaks a loopback test, it indicates a regression.

These tests are fast (no subprocess, no external tool), deterministic, and can run in CI on every commit.

---

## Tasks

### 73.1 Full Session Lifecycle Tests

Verify the complete AP2 session from discovery through teardown.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T1 | `test_ap2_loopback_full_session` | Connect → pair → setup → stream → teardown | All events received: Connected, PairingComplete, StreamingStarted, Disconnected |
| 73-T2 | `test_ap2_loopback_connect_disconnect` | Connect and immediately disconnect (no streaming) | No errors, clean teardown |
| 73-T3 | `test_ap2_loopback_multiple_sessions` | Stream, disconnect, reconnect, stream again | Both streams produce correct audio |
| 73-T4 | `test_ap2_loopback_rapid_sessions` | 10 connect/stream/disconnect cycles | All succeed, no resource leak (check with `ResourceSnapshot`) |
| 73-T5 | `test_ap2_loopback_long_session` | Stream 60s of audio continuously | No drift, no gaps, frequency stable |

---

### 73.2 Pairing Tests

Verify HomeKit pairing behavior end-to-end.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T6 | `test_ap2_loopback_transient_pairing` | Client uses PIN 3939 | Pairing succeeds, streaming works |
| 73-T7 | `test_ap2_loopback_wrong_pin` | Client uses PIN 0000 | `AirPlayError::AuthenticationFailed` |
| 73-T8 | `test_ap2_loopback_persistent_pairing` | Pair, disconnect, reconnect with stored keys | Second connection skips pair-setup, uses pair-verify only. Verify by checking receiver events — no `PairSetupStarted` event on second connection. |
| 73-T9 | `test_ap2_loopback_pairing_storage_corruption` | Store pairing keys, corrupt the file, reconnect | Falls back to full pair-setup |
| 73-T10 | `test_ap2_loopback_pairing_key_rotation` | After many sessions, verify keys are still valid | 20 connect/disconnect cycles with persistent pairing |

**Implementation notes:**
- Persistent pairing tests need a temp directory for `AirPlayConfig::pairing_storage`.
- For T9, write garbage to the pairing file between connections.
- Reference: `src/protocol/pairing/storage.rs`, `src/connection/manager.rs`.

---

### 73.3 Codec Matrix Tests

Verify all codec × sample rate combinations.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T11 | `test_ap2_loopback_pcm_44100` | PCM at 44100 Hz | Bit-exact audio, correct frequency |
| 73-T12 | `test_ap2_loopback_alac_44100` | ALAC at 44100 Hz | Lossless audio, correct frequency |
| 73-T13 | `test_ap2_loopback_pcm_48000` | PCM at 48000 Hz | Audio verified at 48k sample rate |
| 73-T14 | `test_ap2_loopback_alac_48000` | ALAC at 48000 Hz | Lossless audio at 48k |
| 73-T15 | `test_ap2_loopback_codec_full_matrix` | Parametric: all protocol × codec × sample rate | Use `TestMatrix` from Section 72.4 |

**Implementation notes:**
- For 48000 Hz tests, use `TestSineSource::new_with_sample_rate(440.0, 3.0, 48000)`.
- If receiver doesn't support 48000 Hz yet, mark those tests as `#[ignore]` with a TODO.
- Bit-exact comparison: use `compare_audio_exact()` from Section 65 for lossless codecs.

---

### 73.4 Encryption Tests

Verify ChaCha20-Poly1305 encryption end-to-end through the loopback.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T16 | `test_ap2_loopback_encrypted_control` | After pairing, verify RTSP commands are encrypted | Client sends encrypted, receiver decrypts — no errors |
| 73-T17 | `test_ap2_loopback_encrypted_audio` | Verify RTP audio encrypted with shared key | Audio decrypted correctly by receiver, sine wave verified |
| 73-T18 | `test_ap2_loopback_nonce_wraparound` | Stream enough packets to increment nonce significantly | No nonce collision errors |
| 73-T19 | `test_ap2_loopback_hkdf_key_derivation` | Verify both sides derive same keys | Encryption/decryption works bidirectionally |

**Implementation notes:**
- These tests implicitly verify encryption because streaming fails if encryption is broken. The key assertion is that audio arrives correctly.
- To explicitly test encryption, add instrumentation to the receiver that logs when decryption succeeds/fails (via tracing spans).
- Nonce wraparound test: stream for long enough to send >256 packets (each packet increments nonce by 1). At 352 frames/packet at 44100 Hz, 256 packets = ~2 seconds.

---

### 73.5 Playback Control Tests

Verify play/pause/resume/volume/metadata through the full stack.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T20 | `test_ap2_loopback_pause_resume` | Stream, pause, resume, stream more | Gap in audio during pause, audio resumes correctly |
| 73-T21 | `test_ap2_loopback_volume_change` | Set volume during streaming | Receiver event `VolumeChanged`, audio amplitude changes if receiver applies volume |
| 73-T22 | `test_ap2_loopback_volume_range_sweep` | Set volume from 0.0 to 1.0 in 0.1 steps | All volume levels accepted |
| 73-T23 | `test_ap2_loopback_mute_unmute` | Mute during playback, then unmute | Audio silent during mute, resumes after unmute |
| 73-T24 | `test_ap2_loopback_metadata` | Send track metadata during stream | Receiver receives title, artist, album |
| 73-T25 | `test_ap2_loopback_stop_mid_stream` | Call `client.stop()` mid-stream | Receiver gets TEARDOWN, audio output truncated cleanly |

**Implementation notes:**
- Pause/resume: after `client.pause()`, the receiver should stop outputting audio. After `client.play()` (resume), it should restart. Verify by checking audio for a gap of the expected duration.
- Volume: verify the `VolumeChanged` event value. If the receiver applies digital volume, verify amplitude change in audio output.
- Metadata: requires `client.set_metadata(TrackMetadata { ... })`. Verify receiver's `MetadataReceived` event.

---

### 73.6 Stress & Stability Tests

Verify stability under load and unusual conditions.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T26 | `test_ap2_loopback_100_sessions` | 100 sequential connect/stream(1s)/disconnect cycles | All succeed, no resource leak |
| 73-T27 | `test_ap2_loopback_concurrent_clients` | Two clients connect to the same receiver simultaneously | Depends on preemption policy — verify per policy |
| 73-T28 | `test_ap2_loopback_large_packet` | Stream with a modified source that produces oversized frames | Receiver handles gracefully |
| 73-T29 | `test_ap2_loopback_empty_stream` | Connect, send RECORD, but provide no audio data | Receiver detects silence, no crash |
| 73-T30 | `test_ap2_loopback_receiver_restart` | Stop receiver, restart it, reconnect | Second connection works |

**Resource leak detection:** use `ResourceSnapshot` (Section 72.6) before and after T26 to verify no port or task leaks.

---

### 73.7 Audio Quality Deep Tests

More rigorous audio quality checks beyond basic sine wave verification.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 73-T31 | `test_ap2_loopback_stereo_independence` | 440 Hz left, 880 Hz right | Channels independently verified |
| 73-T32 | `test_ap2_loopback_silence_handling` | Stream silence (amplitude 0) for 3s | Receiver outputs near-zero audio |
| 73-T33 | `test_ap2_loopback_max_amplitude` | Stream full-scale sine wave (±32767) | No clipping or distortion detected |
| 73-T34 | `test_ap2_loopback_frequency_sweep` | Stream frequencies: 100, 440, 1000, 4000, 10000 Hz | All frequencies detected correctly in output |
| 73-T35 | `test_ap2_loopback_bit_exact_pcm` | Compare sent PCM with received PCM sample-by-sample | `CompareResult.bit_exact == true` after alignment |
| 73-T36 | `test_ap2_loopback_bit_exact_alac` | Compare sent with received ALAC (lossless) | `CompareResult.bit_exact == true` after alignment |

---

## Acceptance Criteria

- [ ] Full session lifecycle works for AP2 (connect → stream → teardown)
- [ ] All codec × sample rate combinations produce verified audio
- [ ] Pairing works for transient, persistent, and wrong-PIN scenarios
- [ ] Encryption is verified end-to-end
- [ ] Playback control (pause/resume/volume/metadata) works
- [ ] 100-session stress test passes without resource leaks
- [ ] Bit-exact comparison passes for lossless codecs
- [ ] All tests complete in under 60 seconds total (excluding long-stream tests)

---

## References

- `tests/common/loopback.rs` — Section 72 loopback harness
- `tests/common/audio_verify.rs` — Section 65 audio verification
- `airplay2-checklist.md` — AP2 client features
- `airplay2-receiver-checklist.md` — AP2 receiver features
- `src/client/mod.rs` — `AirPlayClient`
- `src/receiver/session_manager.rs` — `SessionManager`
