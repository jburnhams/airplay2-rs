# Section 63: Integration Test Strategy & Roadmap

## Dependencies
- **Section 62**: Integration & Conformance Testing (existing mock-based tests)
- **Section 61**: Testing Infrastructure (mock sender/capture infrastructure)
- **Section 33**: AirPlay 1 Testing
- **Section 45**: AirPlay 1 Receiver Testing

## Overview

This document defines the strategy for comprehensive integration testing of all four components — AirPlay 2 client, AirPlay 2 receiver, AirPlay 1 client, and AirPlay 1 receiver — using both third-party reference implementations and self-loopback tests. The goal is two-layered confidence: first prove correctness against known-good external implementations, then lock in that behaviour with fast regression tests using our own code.

## Principles

1. **Third-party first** — prove our code works with battle-tested implementations before trusting self-tests.
2. **Universal receiver** — our receiver handles both AP1 and AP2 connections; tests never assume a version-specific receiver binary.
3. **Audio-as-truth** — the ultimate assertion is that correct audio arrives. Protocol-level checks are secondary.
4. **Isolation** — each test starts with fresh processes, fresh ports, clean pairing state.
5. **Deterministic audio** — all tests use generated sine waves at known frequencies so output can be mechanically verified without golden files.
6. **CI-friendly** — every test runs headless on Linux, no real audio hardware, no multicast (loopback only).

---

## Test Matrix

### Third-Party Validation (Phase 1)

| Our Component Under Test | Third-Party Tool | Role of Tool | Section |
|---|---|---|---|
| AP2 Client | Python receiver (openairplay) | AP2 receiver | *existing* |
| AP1 Client | shairport-sync | AP1/RAOP receiver | 67 |
| AP2 Client | shairport-sync | AP2 receiver | 68 |
| AP2 Receiver | pyatv | AP2 client/sender | 70 |
| AP1 Receiver | pyatv | AP1/RAOP client | 71 |

### Self-Loopback Regression (Phase 2)

| Client | Receiver | Protocol | Section |
|---|---|---|---|
| Our AP2 Client | Our Universal Receiver | AirPlay 2 | 73 |
| Our AP1 Client | Our Universal Receiver | AirPlay 1/RAOP | 74 |
| Our AP1 Client | Our Universal Receiver | AP1 client → receiver advertising AP2+AP1 | 75 |
| Our AP2 Client | Our Universal Receiver | AP2 client → receiver advertising AP2+AP1 | 75 |

---

## Phase Breakdown

### Phase 0: Shared Infrastructure (Sections 64–65)

**What:** Reusable process management, port allocation, audio verification, and log capture used by all subsequent phases.

**Difficulty:** Medium. The existing `PythonReceiver` in `tests/common/python_receiver.rs` provides a solid pattern to generalise from, but the abstraction must handle three very different subprocesses (Python, C binary, Python+pyatv).

**Risks:**
- Port conflicts between parallel test runs — mitigated by dynamic port allocation throughout.
- Subprocess cleanup on test failure / panic — mitigated by `kill_on_drop(true)` and `Drop` impls.
- Audio file format differences between tools — mitigated by a normalisation layer in the verification framework.

**Deliverables:**
- `tests/common/subprocess.rs` — generic `SubprocessHandle` with health-check, log capture, graceful shutdown.
- `tests/common/ports.rs` — port picker wrapper that reserves ports before handing them to subprocesses.
- `tests/common/audio_verify.rs` — audio analysis routines (amplitude, frequency, continuity, codec-specific checks).

### Phase 1a: shairport-sync (Sections 66–68)

**What:** Build shairport-sync from source in CI, wrap it as a test subprocess, run our AP1 and AP2 clients against it.

**Difficulty:** High. shairport-sync is a C project with many build dependencies (autotools, libpopt, libconfig, libsoxr, ALAC, avahi/dns-sd). Configuration is file-based. It outputs to `stdout` pipe or file backends. Getting it to run headless on loopback with deterministic port assignment is non-trivial.

**Risks:**
- Build environment differences between CI runners — mitigated by Docker container or pinned package versions.
- shairport-sync requires Avahi on Linux — mitigated by using `--with-dns_sd` and a dummy Avahi config, or `--with-tinysvcmdns`.
- Version pinning — shairport-sync 4.x has different AP2 support than 3.x; must pin to known-good version.
- shairport-sync may not expose all audio formats or encryption modes without specific compile flags.

**Deliverables:**
- `tests/common/shairport_sync.rs` — `ShairportSync` subprocess wrapper.
- `tests/shairport_ap1_tests.rs` — AP1 client test suite.
- `tests/shairport_ap2_tests.rs` — AP2 client test suite.
- CI build step for shairport-sync (script or Dockerfile).

### Phase 1b: pyatv (Sections 69–71)

**What:** Use pyatv as an AirPlay client to connect to and stream audio into our receiver.

**Difficulty:** High. pyatv is a library, not a standalone binary. We need a Python driver script that uses pyatv's API to discover our receiver on loopback, pair, stream a known audio file, and exit. pyatv's AirPlay audio streaming support has limitations — it primarily supports stream_file and push-based audio. Testing on loopback with mDNS requires careful interface configuration.

**Risks:**
- pyatv may not support all AirPlay features we want to test (e.g., ALAC selection, specific volume ranges) — mitigated by testing what it supports and documenting gaps.
- mDNS on loopback — pyatv uses zeroconf which may not work on loopback without configuration. May need to use unicast or inject the service record directly.
- Our receiver must be functional enough to accept pyatv connections — if receiver is incomplete, tests will block on receiver bugs, not client bugs. This is a feature, not a bug, but sequencing matters.
- pyatv version API changes — pin version.

**Deliverables:**
- `tests/pyatv/driver_ap2.py` — Python script driving pyatv as AP2 client.
- `tests/pyatv/driver_ap1.py` — Python script driving pyatv as AP1/RAOP client.
- `tests/common/pyatv.rs` — `PyAtvDriver` subprocess wrapper.
- `tests/pyatv_ap2_receiver_tests.rs` — AP2 receiver test suite.
- `tests/pyatv_ap1_receiver_tests.rs` — AP1 receiver test suite.

### Phase 2: Self-Loopback (Sections 72–75)

**What:** Run our client and receiver in the same test process, connected via loopback, for fast regression testing.

**Difficulty:** Medium. The in-process approach avoids subprocess complexity, but requires careful async task management — the receiver runs as a background task while the client connects to it. Both share the same tokio runtime. Port allocation, mDNS advertisement on loopback, and teardown ordering all need care.

**Risks:**
- Shared bugs — if both client and receiver have the same protocol misunderstanding, loopback tests pass but real devices fail. This is exactly why Phase 1 exists.
- Test flakiness from timing — receiver may not be ready when client connects. Mitigated by health-check polling.
- mDNS advertisement + discovery on loopback may not work on all platforms — mitigated by direct connection (skip discovery, construct `AirPlayDevice` manually).

**Deliverables:**
- `tests/common/loopback.rs` — `LoopbackHarness` that starts receiver as tokio task and returns connection info.
- `tests/loopback_ap2_tests.rs` — AP2 loopback suite.
- `tests/loopback_ap1_tests.rs` — AP1 loopback suite.
- `tests/loopback_cross_tests.rs` — cross-protocol and mixed-version suite.

### Phase 3: CI/CD (Section 76)

**What:** GitHub Actions workflows that run all integration test suites, with proper dependency installation, Docker images, artifact capture, and parallel execution.

**Difficulty:** Medium. Main challenge is shairport-sync build caching and Docker layer management.

**Deliverables:**
- `.github/workflows/integration-shairport.yml`
- `.github/workflows/integration-pyatv.yml`
- `.github/workflows/integration-loopback.yml`
- `docker/shairport-sync/Dockerfile`
- Updated `.github/workflows/integration.yml` (existing, for Python receiver tests).

---

## Dependency Graph Between Sections

```
63 (Strategy) ─── this document
 │
 ├── 64 (Subprocess Framework) ◄── used by 66, 69
 │    └── depends on: tests/common/python_receiver.rs (existing pattern)
 │
 ├── 65 (Audio Verification) ◄── used by 67, 68, 70, 71, 73, 74, 75
 │    └── depends on: tests/common/python_receiver.rs::verify_sine_wave_quality (existing)
 │
 ├── 66 (shairport-sync Setup) ◄── depends on 64
 │    ├── 67 (AP1 Client vs shairport-sync) ◄── depends on 65, 66
 │    └── 68 (AP2 Client vs shairport-sync) ◄── depends on 65, 66
 │
 ├── 69 (pyatv Setup) ◄── depends on 64
 │    ├── 70 (AP2 Receiver vs pyatv) ◄── depends on 65, 69
 │    └── 71 (AP1 Receiver vs pyatv) ◄── depends on 65, 69
 │
 ├── 72 (Loopback Infrastructure) ◄── depends on 65
 │    ├── 73 (AP2 Loopback) ◄── depends on 72
 │    ├── 74 (AP1 Loopback) ◄── depends on 72
 │    └── 75 (Cross-Protocol) ◄── depends on 72
 │
 └── 76 (CI/CD) ◄── depends on all above
```

**Parallelism:** Sections 66–68 (shairport-sync track) and 69–71 (pyatv track) can be developed in parallel. Section 72–75 (loopback) can also proceed in parallel once Section 65 is complete, since it has no external dependency.

---

## Implementation Priority

| Priority | Section | Rationale |
|---|---|---|
| P0 | 64, 65 | Foundation for everything else |
| P1 | 66, 67 | AP1 client is likely most complete; shairport-sync is well-documented |
| P1 | 69, 70 | AP2 receiver validation is critical |
| P2 | 68 | AP2 client vs shairport-sync — depends on shairport-sync AP2 support |
| P2 | 71 | AP1 receiver vs pyatv — depends on pyatv RAOP support |
| P2 | 72, 73, 74 | Loopback tests — best started after Phase 1 validates correctness |
| P3 | 75 | Cross-protocol — depends on universal receiver maturity |
| P3 | 76 | CI/CD — can be built incrementally as test suites land |

---

## Test Naming Convention

All integration test files follow:
```
tests/{tool}_{protocol}_{component}_tests.rs
```

Examples:
- `tests/shairport_ap1_client_tests.rs`
- `tests/pyatv_ap2_receiver_tests.rs`
- `tests/loopback_ap2_tests.rs`
- `tests/loopback_cross_tests.rs`

All test functions follow:
```
test_{feature}_{scenario}[_{variant}]
```

Examples:
- `test_pcm_streaming_44100_stereo`
- `test_alac_streaming_end_to_end`
- `test_wrong_pin_rejected`
- `test_volume_change_during_playback`

---

## Success Criteria

- [ ] Every row in the test matrix has at least one passing end-to-end test
- [ ] Audio quality verified (correct frequency, amplitude, continuity) for every codec path
- [ ] Both AP1 and AP2 pairing/auth paths exercised against third-party tools
- [ ] Loopback tests cover all codec × encryption × sample-rate combinations
- [ ] All tests run in CI without manual intervention
- [ ] Test failures produce diagnostic artifacts (logs, audio dumps, RTP captures)
- [ ] No test depends on real audio hardware or multicast networking

---

## References

- [shairport-sync GitHub](https://github.com/mikebrady/shairport-sync)
- [pyatv documentation](https://pyatv.dev/)
- [openairplay/airplay2-receiver](https://github.com/openairplay/airplay2-receiver)
- [Section 62: Integration & Conformance Testing](./62-integration-conformance-testing.md)
- [Section 61: Testing Infrastructure](./61-testing-infrastructure.md)
- `tests/common/python_receiver.rs` — existing subprocess pattern
- `.github/workflows/integration.yml` — existing CI pattern
