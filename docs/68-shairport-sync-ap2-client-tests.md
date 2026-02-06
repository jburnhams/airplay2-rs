# Section 68: AP2 Client vs shairport-sync — Tests

## Dependencies
- **Section 66**: shairport-sync Build, Configuration & Subprocess Wrapper
- **Section 65**: Audio Verification & Analysis Framework
- **Section 64**: Subprocess Management Framework
- `airplay2-checklist.md` — checklist items verified by these tests

## Overview

This section defines integration tests that exercise our AirPlay 2 client against shairport-sync configured with AirPlay 2 support. This provides a second independent validation of our AP2 client (complementing the existing Python receiver tests), using a completely different receiver implementation written in C. Differences in how shairport-sync and the Python receiver handle AP2 will expose protocol assumptions in our client.

## Important: AirPlay 2 Prerequisites in shairport-sync

shairport-sync's AirPlay 2 support requires:
1. **NQPTP** (Network Quality PTP) daemon running — a separate process that handles PTP timing.
2. Compile with `--with-airplay-2` flag.
3. `libplist`, `libsodium`, `libgcrypt` installed.
4. A valid configuration with AP2-specific fields.

**This means Phase 1a AP2 tests are harder than AP1 tests.** Consider implementing Section 67 (AP1) first, then extending to AP2.

**NQPTP management:** NQPTP must run alongside shairport-sync. It is a separate daemon that needs root privileges (for hardware timestamping) or can run in a degraded mode. On CI, we will run it without hardware timestamping.

---

## Tasks

### 68.1 NQPTP Setup

**File:** `tests/common/nqptp.rs`

NQPTP is required for shairport-sync AirPlay 2 mode.

**Build:** NQPTP is a small C project from `https://github.com/mikebrady/nqptp`.

Build steps:
1. Clone at pinned version.
2. `autoreconf -fi && ./configure && make`
3. Binary at `target/nqptp/bin/nqptp`

**Struct: `Nqptp`**

Fields:
- `handle: SubprocessHandle`

Methods:
- `async fn start() -> Result<Self, NqptpError>` — spawn NQPTP with `ready_pattern` matching its startup message. On CI (where we may not have root), run with `--no-daemon` flag.
- `async fn stop(self) -> Result<(), NqptpError>`

**Uncertainties:**
- NQPTP may require root for raw socket access. On CI with Docker, this is fine. Without Docker, may need `sudo` or capability flags. If NQPTP cannot start, AP2 shairport-sync tests should be skipped with a clear message.
- NQPTP's interface binding — may need `--interface lo` flag.

---

### 68.2 AP2-Specific shairport-sync Configuration

**File:** `tests/common/shairport_sync.rs` (extension to Section 66)

Extend `ShairportConfig` with AP2-specific fields:

Additional fields for AP2 mode:
- `airplay2_enabled: true`
- `ptp_enabled: true`

Additional config template section when `airplay2_enabled`:
```
// No extra config section needed — shairport-sync auto-detects AP2
// when compiled with --with-airplay-2 and NQPTP is running.
// But we may need to set feature flags in the TXT record.
```

**Device config for AP2:** when `airplay2_enabled`, `device_config()` must set:
- `capabilities.airplay2: true`
- `capabilities.supports_homekit_pairing: true`
- `capabilities.supports_transient_pairing: true`
- `raop_port: None` (AP2 uses the main port, not a separate RAOP port)

---

### 68.3 HomeKit Pairing Tests

**Checklist items covered:**
- `airplay2-checklist.md` → Pairing and Authentication → Transient Pairing
- `airplay2-checklist.md` → Pairing and Authentication → Standard HomeKit Pairing

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T1 | `test_ap2_transient_pairing_shairport` | Connect using transient pairing (PIN 3939) | `client.is_connected() == true`, pair-setup completes |
| 68-T2 | `test_ap2_pair_verify_shairport` | Transient pair-setup, then pair-verify | Both phases complete, encrypted channel established |
| 68-T3 | `test_ap2_wrong_pin_rejected` | Attempt pairing with wrong PIN | `AirPlayError::AuthenticationFailed` |
| 68-T4 | `test_ap2_persistent_pairing_shairport` | Pair, disconnect, reconnect using stored keys | Second connection uses pair-verify only (no pair-setup) |
| 68-T5 | `test_ap2_srp_key_agreement` | Verify SRP6a key agreement produces valid shared secret | Pairing succeeds, subsequent encrypted messages decode correctly |

**Implementation notes:**
- shairport-sync supports transient pairing with code 3939 by default.
- For persistent pairing tests, configure `AirPlayConfig::builder().pairing_storage(temp_path)`.
- Verify pairing works by checking that encrypted control channel messages succeed after pair-verify.
- Reference: `src/protocol/pairing/` for pairing implementation, `src/connection/manager.rs` for pairing flow.

**Uncertainties:**
- shairport-sync's SRP implementation may have subtle differences from the Python receiver's. This is exactly what we want to test — our client must work with both.
- shairport-sync may require specific feature flags for pairing. Check shairport-sync docs for `--with-convolution` or similar flags that affect pairing behavior.

---

### 68.4 Encrypted Control Channel Tests

**Checklist items covered:**
- `airplay2-checklist.md` → Encryption and Key Derivation → Session Encryption
- `airplay2-checklist.md` → Encryption and Key Derivation → Session Key Management

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T6 | `test_ap2_chacha20_encrypted_control` | After pairing, verify all RTSP commands are encrypted | shairport-sync processes commands correctly (no decrypt errors in logs) |
| 68-T7 | `test_ap2_hkdf_key_derivation` | Verify HKDF-SHA-512 produces keys compatible with shairport-sync | Encrypted messages accepted by receiver |
| 68-T8 | `test_ap2_nonce_increment` | Send multiple encrypted commands, verify nonce increments correctly | No replay/nonce errors in shairport-sync logs |
| 68-T9 | `test_ap2_bidirectional_encryption` | Verify client can decrypt receiver's encrypted responses | Client successfully parses encrypted SETUP response |

**Implementation notes:**
- After pair-verify, the RTSP connection switches to HAP-encrypted framing (ChaCha20-Poly1305).
- Verification: if any encrypted message is malformed, shairport-sync will reject it and log an error. Absence of errors in logs confirms correct encryption.
- Reference: `src/protocol/crypto/chacha.rs`, `src/connection/manager.rs` (encrypted channel).

---

### 68.5 AP2 SETUP Phase Tests

**Checklist items covered:**
- `airplay2-checklist.md` → Protocol Stack → RTSP → SETUP

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T10 | `test_ap2_setup_phase1_timing` | Send SETUP phase 1 (timing/event channels) | Response contains event port, timing protocol acknowledged |
| 68-T11 | `test_ap2_setup_phase2_audio` | Send SETUP phase 2 (audio stream) | Response contains data port and control port |
| 68-T12 | `test_ap2_setup_ptp_timing` | Negotiate PTP timing in phase 1 | shairport-sync + NQPTP accept PTP timing |
| 68-T13 | `test_ap2_setup_audio_format_pcm` | Request PCM format in phase 2 | shairport-sync accepts and prepares for PCM reception |
| 68-T14 | `test_ap2_setup_audio_format_alac` | Request ALAC format in phase 2 | shairport-sync accepts and prepares for ALAC reception |
| 68-T15 | `test_ap2_setup_shared_key` | Verify `shk` (shared encryption key) in phase 2 body | shairport-sync accepts key for RTP decryption |

**Implementation notes:**
- AP2 SETUP is multi-phase: phase 1 establishes timing/event, phase 2 establishes audio.
- The binary plist body format must exactly match what shairport-sync expects.
- Reference: `src/connection/manager.rs` for SETUP flow.

---

### 68.6 AP2 Audio Streaming Tests

**Checklist items covered:**
- `airplay2-checklist.md` → Audio Codec Support → PCM, ALAC
- `airplay2-checklist.md` → RTP/RTCP → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T16 | `test_ap2_pcm_streaming_shairport` | Stream 3s of 440 Hz PCM to shairport-sync (AP2 mode) | Frequency, amplitude, duration verified |
| 68-T17 | `test_ap2_alac_streaming_shairport` | Stream 3s of 440 Hz ALAC to shairport-sync (AP2 mode) | Frequency match, lossless verified |
| 68-T18 | `test_ap2_encrypted_rtp_audio` | Verify RTP audio packets are encrypted with ChaCha20 | Audio correctly decoded by shairport-sync |
| 68-T19 | `test_ap2_stereo_channels` | Stream distinct L/R channels | Both channels verified independently |
| 68-T20 | `test_ap2_long_stream_30s` | Stream 30s continuous audio | No gaps, no drift, stable frequency |
| 68-T21 | `test_ap2_stream_then_pause_resume` | Stream, pause, resume, verify audio continuity | Gap during pause, audio resumes correctly |

**Comparison with Python receiver tests:** these tests mirror the existing `test_pcm_streaming_end_to_end` and `test_alac_streaming_end_to_end` from `tests/integration_tests.rs`, but against shairport-sync instead of the Python receiver. Any differences in behavior indicate a protocol compatibility issue in our client.

---

### 68.7 AP2 Volume, Metadata & Control Tests

**Checklist items covered:**
- `airplay2-checklist.md` → Metadata and Control → all items

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T22 | `test_ap2_volume_during_stream` | Set volume during streaming | shairport-sync logs volume change |
| 68-T23 | `test_ap2_pause_resume` | Pause and resume playback | shairport-sync logs rate change (0.0 → 1.0) |
| 68-T24 | `test_ap2_get_info` | Send GET /info, verify binary plist response | Response contains required fields (see airplay2-checklist.md) |
| 68-T25 | `test_ap2_feedback_heartbeat` | Send /feedback during stream | No session timeout |

---

### 68.8 Error Handling & Edge Cases

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 68-T26 | `test_ap2_nqptp_not_running` | Start shairport-sync without NQPTP, attempt AP2 connection | Graceful failure, clear error message |
| 68-T27 | `test_ap2_server_killed_mid_stream` | Kill shairport-sync during AP2 streaming | Client detects disconnect |
| 68-T28 | `test_ap2_reconnect_after_crash` | Kill and restart shairport-sync, reconnect | Second connection succeeds |
| 68-T29 | `test_ap2_setup_phase2_without_phase1` | Skip phase 1, send phase 2 directly | shairport-sync rejects, our client handles error |
| 68-T30 | `test_ap2_rapid_connect_disconnect` | 10 connect/disconnect cycles in AP2 mode | All succeed, no resource leaks |

---

## Comparison Matrix: Python Receiver vs shairport-sync

| Feature | Python Receiver (existing) | shairport-sync (this section) |
|---|---|---|
| Language | Python | C |
| AP2 support | Yes | Yes (with NQPTP) |
| AP1 support | No | Yes |
| Pairing | Transient only | Transient + persistent |
| Audio capture | File sink | Pipe backend |
| Encryption | ChaCha20 | ChaCha20 |
| PTP timing | Basic | Full (via NQPTP) |
| Metadata | Partial | Full |
| Password | No | Yes |

This matrix highlights why testing against both is valuable — shairport-sync exercises more protocol features than the Python receiver.

---

## Acceptance Criteria

- [ ] AP2 transient pairing works against shairport-sync
- [ ] Encrypted RTSP control channel established successfully
- [ ] SETUP phases 1 and 2 complete correctly
- [ ] PCM and ALAC streaming produce verified audio output
- [ ] Volume and metadata commands accepted by shairport-sync
- [ ] Long-stream test shows no drift or gaps
- [ ] Error handling tests don't crash or hang
- [ ] NQPTP starts and stops cleanly in CI

---

## References

- `airplay2-checklist.md` — AP2 client checklist
- [shairport-sync AirPlay 2 support](https://github.com/mikebrady/shairport-sync/blob/master/AIRPLAY2.md)
- [NQPTP](https://github.com/mikebrady/nqptp)
- `tests/integration_tests.rs` — existing Python receiver tests for comparison
- `src/connection/manager.rs` — AP2 connection flow
- `src/protocol/pairing/` — HomeKit pairing
- `src/protocol/crypto/` — encryption primitives
