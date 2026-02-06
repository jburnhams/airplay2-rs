# Section 75: Cross-Protocol & Mixed-Version Tests

## Dependencies
- **Section 72**: Loopback Test Infrastructure
- **Section 73**: AP2 Loopback Tests
- **Section 74**: AP1 Loopback Tests
- **Section 65**: Audio Verification & Analysis Framework

## Overview

Our receiver is universal — it handles both AirPlay 1 (RAOP) and AirPlay 2 connections on the same instance. This section tests scenarios that span protocol boundaries: AP1 clients connecting to a receiver that also advertises AP2, AP2 clients connecting to a receiver that also accepts RAOP, sequential sessions switching between protocols, and protocol negotiation edge cases.

These tests are unique to our library (third-party tools test one protocol at a time) and verify that the multi-protocol support works correctly without interference.

---

## Tasks

### 75.1 AP1 Client → Universal Receiver

The receiver advertises both `_airplay._tcp` and `_raop._tcp`. An AP1 client connects via the RAOP path. Verify that the AP2 capability doesn't interfere with the AP1 session.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T1 | `test_ap1_client_to_universal_receiver` | AP1 client (`ForceRaop`) connects to universal receiver | RAOP session established, audio verified |
| 75-T2 | `test_ap1_client_ignores_ap2_features` | Universal receiver has AP2 features in TXT, AP1 client ignores them | Client uses RAOP path, no AP2 negotiation attempted |
| 75-T3 | `test_ap1_pcm_on_universal` | AP1 PCM streaming to universal receiver | Audio verified |
| 75-T4 | `test_ap1_alac_on_universal` | AP1 ALAC streaming to universal receiver | Audio verified |
| 75-T5 | `test_ap1_encrypted_on_universal` | AP1 RSA+AES encrypted stream to universal receiver | Decryption works, audio verified |

**Implementation notes:**
- Configure `ReceiverTestConfig { enable_ap1: true, enable_ap2: true, ... }`.
- Configure client with `PreferredProtocol::ForceRaop`.
- The receiver must correctly route incoming RTSP requests based on content — RAOP clients send `ANNOUNCE` with SDP, AP2 clients send binary plist bodies.

---

### 75.2 AP2 Client → Universal Receiver

Same as 75.1 but with AP2 client connecting to the universal receiver that also supports AP1.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T6 | `test_ap2_client_to_universal_receiver` | AP2 client (`ForceAirPlay2`) connects to universal receiver | AP2 session established, audio verified |
| 75-T7 | `test_ap2_client_ignores_raop` | Universal receiver has RAOP capability, AP2 client ignores it | Client uses AP2 path |
| 75-T8 | `test_ap2_pairing_on_universal` | AP2 pairing works when receiver also supports AP1 | Pairing succeeds, encrypted channel works |
| 75-T9 | `test_ap2_pcm_on_universal` | AP2 PCM streaming to universal receiver | Audio verified |
| 75-T10 | `test_ap2_alac_on_universal` | AP2 ALAC streaming to universal receiver | Audio verified |

---

### 75.3 Protocol Auto-Selection

When the client uses `PreferredProtocol::PreferAirPlay2` or `PreferRaop`, it should automatically select the best available protocol.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T11 | `test_auto_selects_ap2_when_available` | Universal receiver, client `PreferAirPlay2` | Client chooses AP2, `SelectedProtocol::AirPlay2` |
| 75-T12 | `test_auto_selects_raop_when_ap2_unavailable` | Receiver with only AP1, client `PreferAirPlay2` | Client falls back to RAOP |
| 75-T13 | `test_prefer_raop_uses_raop` | Universal receiver, client `PreferRaop` | Client chooses RAOP |
| 75-T14 | `test_force_ap2_fails_on_ap1_only` | Receiver with only AP1, client `ForceAirPlay2` | `AirPlayError` indicating AP2 not supported |
| 75-T15 | `test_force_raop_fails_on_ap2_only` | Receiver with only AP2, client `ForceRaop` | `AirPlayError` indicating RAOP not supported |
| 75-T16 | `test_auto_select_streams_successfully` | Client auto-selects protocol and streams audio | Audio verified regardless of which protocol was chosen |

**Implementation notes:**
- Protocol selection logic is in `src/client/protocol.rs::select_protocol()`.
- The `AirPlayDevice` must have correct `capabilities` and `raop_capabilities` set for the selection to work.
- For T12, configure `ReceiverTestConfig { enable_ap1: true, enable_ap2: false }`.

---

### 75.4 Sequential Mixed-Protocol Sessions

The receiver handles one session at a time. Test that it correctly switches between AP1 and AP2 sessions.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T17 | `test_ap2_then_ap1_session` | AP2 stream → teardown → AP1 stream | Both sessions complete, both audio outputs verified |
| 75-T18 | `test_ap1_then_ap2_session` | AP1 stream → teardown → AP2 stream | Both sessions complete |
| 75-T19 | `test_alternating_protocols_10x` | Alternate AP1/AP2 for 10 sessions | All succeed, no state leakage between sessions |
| 75-T20 | `test_ap2_then_ap1_different_codecs` | AP2+ALAC → teardown → AP1+PCM | Both audio outputs correct for their respective codecs |
| 75-T21 | `test_ap1_encrypted_then_ap2` | AP1 with RSA+AES → teardown → AP2 with ChaCha20 | Encryption state fully reset between sessions |

**Key verification:** state from a previous protocol session must not leak into the next. Specifically:
- Encryption keys from AP1 (AES-128) must not interfere with AP2 (ChaCha20-Poly1305).
- SDP parameters from AP1 ANNOUNCE must not persist into AP2 SETUP.
- Port allocations must be fully released before the next session.

---

### 75.5 Concurrent Connection Attempts (Different Protocols)

Test what happens when AP1 and AP2 clients try to connect simultaneously.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T22 | `test_ap1_while_ap2_streaming` | AP2 is streaming, AP1 client tries to connect | AP1 rejected (or AP2 preempted, per policy) |
| 75-T23 | `test_ap2_while_ap1_streaming` | AP1 is streaming, AP2 client tries to connect | AP2 rejected or AP1 preempted |
| 75-T24 | `test_preempt_ap1_with_ap2` | Configure preemption, AP2 client preempts AP1 | AP1 session terminated, AP2 session starts |
| 75-T25 | `test_reject_concurrent_same_protocol` | Two AP2 clients try to connect | Second rejected |

**Implementation notes:**
- Preemption behavior depends on `SessionManagerConfig::preemption_policy`.
- Test with all three policies: `Reject`, `AllowPreempt`, `Queue` (if implemented).
- Reference: `src/receiver/session_manager.rs`.

---

### 75.6 Service Advertisement Correctness

When the universal receiver advertises both protocols, verify the advertisements are correct and independent.

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T26 | `test_dual_advertisement` | Universal receiver advertises both `_airplay._tcp` and `_raop._tcp` | Both services discoverable |
| 75-T27 | `test_ap2_txt_records_independent` | AP2 TXT records don't contain RAOP-specific fields | No `tp=UDP` in `_airplay._tcp` TXT |
| 75-T28 | `test_raop_txt_records_independent` | RAOP TXT records don't contain AP2-specific fields | No `pk` in `_raop._tcp` TXT (or appropriate for RAOP) |
| 75-T29 | `test_raop_name_format` | RAOP service name is `MAC@Name`, AP2 is just `Name` | Both formats correct |
| 75-T30 | `test_busy_flag_across_protocols` | During AP1 session, both services show busy | `sf` flag updated on both advertisements |

---

### 75.7 Protocol Feature Interaction Edge Cases

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T31 | `test_ap1_client_with_ap2_device_capabilities` | Client receives both AP1 and AP2 capabilities, selects AP1 | RAOP path used correctly |
| 75-T32 | `test_volume_units_consistent` | Set volume via AP1 (-15.0 dB text format), then via AP2 (binary plist). Verify same internal representation | Volume events show consistent dB values |
| 75-T33 | `test_codec_negotiation_ap1_vs_ap2` | AP1 announces ALAC via SDP, AP2 requests ALAC via binary plist. Both decoded identically | Same audio output quality |
| 75-T34 | `test_session_id_unique_across_protocols` | AP1 and AP2 sessions get different session IDs | No session ID collision |
| 75-T35 | `test_receiver_state_clean_after_ap1_crash` | AP1 client crashes, receiver cleans up, AP2 client connects | AP2 session works correctly, no AP1 state leaking |

---

### 75.8 Stress & Stability Tests

#### Test Cases

| ID | Test | Description | Key Assertions |
|---|---|---|---|
| 75-T36 | `test_50_alternating_sessions` | 50 sessions alternating AP1/AP2 | All succeed, no resource leak |
| 75-T37 | `test_rapid_protocol_switching` | Connect/disconnect switching protocols every 500ms for 30 seconds | Stable, no crash |
| 75-T38 | `test_long_running_receiver` | Receiver runs for 5 minutes handling mixed sessions | No memory growth (check RSS before/after) |

**Resource monitoring:** for T38, sample RSS (resident set size) via `/proc/self/statm` at start and end. Allow no more than 10% growth.

---

## Acceptance Criteria

- [ ] AP1 client works correctly against universal receiver
- [ ] AP2 client works correctly against universal receiver
- [ ] Protocol auto-selection chooses correct protocol based on receiver capabilities
- [ ] Sequential mixed-protocol sessions work without state leakage
- [ ] Concurrent connection attempts handled per preemption policy
- [ ] Dual service advertisement correct for both protocols
- [ ] Volume and codec handling consistent across protocols
- [ ] 50-session alternating stress test passes
- [ ] No resource leaks in any cross-protocol scenario

---

## References

- `src/client/protocol.rs` — `select_protocol()`, `PreferredProtocol`
- `src/receiver/session_manager.rs` — `SessionManager`, `PreemptionPolicy`
- `src/discovery/advertiser.rs` — dual service advertisement
- `tests/common/loopback.rs` — Section 72 loopback infrastructure
