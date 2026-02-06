# Section 72: Loopback Test Infrastructure

## Dependencies
- **Section 65**: Audio Verification & Analysis Framework
- **Section 70**: AP2 Receiver vs pyatv (for `ReceiverHarness` and `FileAudioSink`)
- **Section 63**: Integration Test Strategy

## Overview

Loopback tests run our client and receiver in the same process, connected via loopback networking (`127.0.0.1`). These are the fastest integration tests (no subprocesses, no Python, no C builds) and form the primary regression test suite. They verify that our own client and receiver implementations are mutually compatible and that changes to one don't break the other.

Loopback tests are Phase 2 — they must be implemented after Phase 1 (third-party validation) confirms that our individual components are correct. Without Phase 1, loopback tests can only prove self-consistency, not correctness.

## Key Design Decisions

1. **In-process, not subprocess** — both client and receiver run as tokio tasks in the same test process. This is faster (no spawn overhead) and easier to debug (shared logs, breakpoints work).

2. **Universal receiver** — our receiver is a single binary that handles both AP1 and AP2 connections. Loopback tests exercise both protocols against the same receiver instance.

3. **Skip mDNS** — construct `AirPlayDevice` manually with known address/port. Discovery is tested separately in Sections 70-T1 and 71-T1.

4. **File audio sink** — receiver writes to a file, client generates from `TestSineSource`. After streaming, verify the file contents.

5. **Deterministic ports** — use port allocator (Section 64) to avoid conflicts. Never hardcode ports.

---

## Tasks

### 72.1 LoopbackHarness

**File:** `tests/common/loopback.rs`

The central harness that orchestrates a loopback test: start receiver, create client, run test, verify audio, tear down.

**Struct: `LoopbackHarness`**

Fields:
- `receiver: ReceiverHarness` — from Section 70.1
- `client: AirPlayClient` — from `src/client/mod.rs`
- `device: AirPlayDevice` — target device info pointing to our receiver
- `audio_sink_path: PathBuf`
- `config: LoopbackConfig`

**Struct: `LoopbackConfig`**

Fields:
- `receiver_config: ReceiverTestConfig` — receiver settings
- `client_config: AirPlayConfig` — client settings
- `protocol: LoopbackProtocol` — which protocol to test
- `codec: AudioCodec` — which codec to use
- `test_frequency: f32` — sine wave frequency (default: 440.0)
- `stream_duration: f32` — seconds of audio to stream (default: 3.0)
- `verify_audio: bool` — whether to verify audio output (default: true)

**Enum: `LoopbackProtocol`**
- `AirPlay2` — AP2 only
- `Raop` — AP1/RAOP only
- `Auto` — let protocol negotiation decide (tests `select_protocol()`)

Methods:

**`async fn new(config: LoopbackConfig) -> Result<Self, LoopbackError>`**

Steps:
1. Build `ReceiverTestConfig` from `config.receiver_config`:
   - Enable AP1 and/or AP2 based on `config.protocol`.
   - Set audio sink path to temp directory.
   - Set port to 0 (auto-allocate).
2. Start `ReceiverHarness::start(receiver_config)`.
3. Build `AirPlayConfig` from `config.client_config`:
   - Set `audio_codec` from `config.codec`.
   - Set protocol preference based on `config.protocol`.
   - Set `pin("3939")` for AP2 transient pairing.
4. Create `AirPlayClient::new(client_config)`.
5. Build `AirPlayDevice` from receiver harness:
   - `addresses: vec!["127.0.0.1"]`
   - `port: receiver.port()`
   - `capabilities` matching receiver config.
6. Return harness.

**`async fn run_stream_test(&mut self) -> Result<LoopbackResult, LoopbackError>`**

Convenience method that runs the standard stream test pattern:
1. `client.connect(&device).await?`
2. Create `TestSineSource::new(frequency, duration)`.
3. `client.stream_audio(source).await?`
4. `client.disconnect().await?`
5. Brief pause (200ms) for receiver to flush audio sink.
6. Stop receiver, read audio file.
7. If `verify_audio`, run `SineWaveCheck::verify()`.
8. Return `LoopbackResult`.

**`async fn teardown(self) -> Result<LoopbackResult, LoopbackError>`**

Explicit teardown — stops receiver, collects output. Call this instead of `run_stream_test` when the test manages the client/receiver lifecycle manually.

**Struct: `LoopbackResult`**

Fields:
- `audio: Option<RawAudio>` — received audio (from Section 65)
- `sine_result: Option<SineWaveResult>` — verification result
- `receiver_events: Vec<ReceiverEvent>` — events from receiver
- `client_state: ClientState` — final client state
- `stream_duration: Duration` — actual streaming time
- `audio_sink_bytes: u64` — bytes written to audio file

---

### 72.2 Protocol Configuration Helpers

**File:** `tests/common/loopback.rs`

Helper functions to create pre-configured `LoopbackConfig` for common scenarios.

**`fn ap2_pcm_config() -> LoopbackConfig`**

Default AP2 + PCM loopback config. Used by most AP2 tests.

**`fn ap2_alac_config() -> LoopbackConfig`**

AP2 + ALAC. Sets `codec: AudioCodec::Alac`.

**`fn ap1_pcm_config() -> LoopbackConfig`**

AP1/RAOP + PCM. Sets `protocol: LoopbackProtocol::Raop`, appropriate encryption.

**`fn ap1_alac_config() -> LoopbackConfig`**

AP1/RAOP + ALAC.

**`fn universal_config() -> LoopbackConfig`**

Receiver advertises both AP1 and AP2. Client uses `Auto` protocol. Tests protocol negotiation.

**`fn custom_config(protocol: LoopbackProtocol, codec: AudioCodec, duration: f32) -> LoopbackConfig`**

Parametric config builder for matrix tests.

---

### 72.3 Client Factory

**File:** `tests/common/loopback.rs`

**Function: `fn create_client_for_protocol(protocol: LoopbackProtocol, codec: AudioCodec) -> AirPlayClient`**

Constructs an `AirPlayClient` with the appropriate config:
- For `AirPlay2`: `PreferredProtocol::ForceAirPlay2`, pin "3939".
- For `Raop`: `PreferredProtocol::ForceRaop`, no pin.
- For `Auto`: `PreferredProtocol::PreferAirPlay2`, pin "3939".

**Function: `fn create_device_for_receiver(receiver: &ReceiverHarness, protocol: LoopbackProtocol) -> AirPlayDevice`**

Constructs an `AirPlayDevice` from the receiver harness with correct capabilities:
- For `AirPlay2`: `capabilities.airplay2: true`, no `raop_port`.
- For `Raop`: `raop_port: Some(port)`, `raop_capabilities` populated.
- For `Auto`: both AP2 and RAOP capabilities set.

---

### 72.4 Parametric Test Matrix Runner

**File:** `tests/common/loopback.rs`

Many loopback tests are parametric — the same test logic run across multiple protocol/codec/format combinations. Rather than writing N separate test functions, provide a matrix runner.

**Struct: `TestMatrix`**

Fields:
- `protocols: Vec<LoopbackProtocol>`
- `codecs: Vec<AudioCodec>`
- `sample_rates: Vec<u32>` — 44100, 48000
- `durations: Vec<f32>` — e.g., 1.0, 3.0, 10.0

**Method: `fn combinations(&self) -> Vec<LoopbackConfig>`**

Returns cartesian product of all parameters as configs.

**Method: `async fn run_all(&self) -> Vec<(LoopbackConfig, Result<LoopbackResult, LoopbackError>)>`**

Runs each combination sequentially (not parallel, due to port constraints) and collects results.

**Usage in tests:**
```rust
#[tokio::test]
#[ignore]
async fn test_codec_matrix() {
    let matrix = TestMatrix {
        protocols: vec![AirPlay2, Raop],
        codecs: vec![Pcm, Alac],
        sample_rates: vec![44100],
        durations: vec![3.0],
    };
    let results = matrix.run_all().await;
    for (config, result) in results {
        result.expect(&format!("Failed for {:?} {:?}", config.protocol, config.codec));
    }
}
```

---

### 72.5 Event Assertion Helpers

**File:** `tests/common/loopback.rs`

**Function: `fn assert_events_contain(events: &[ReceiverEvent], expected: &[EventType])`**

Verify that the event log contains all expected event types in order.

**Function: `fn assert_no_errors(events: &[ReceiverEvent])`**

Verify no error events in the log.

**Function: `async fn wait_for_receiver_streaming(harness: &mut LoopbackHarness, timeout: Duration) -> bool`**

Wait until the receiver reports `StreamingStarted` event. Returns false on timeout.

**Function: `async fn wait_for_receiver_idle(harness: &mut LoopbackHarness, timeout: Duration) -> bool`**

Wait until the receiver has no active sessions.

---

### 72.6 Resource Leak Detection

**File:** `tests/common/loopback.rs`

Loopback tests should verify no resource leaks across repeated runs.

**Function: `fn count_open_ports() -> usize`**

Parse `/proc/net/tcp` (Linux) or `lsof -i` (macOS) to count open TCP/UDP sockets in the test port range. Compare before and after a test to detect leaks.

**Function: `fn count_tokio_tasks() -> usize`**

Use `tokio::runtime::Handle::current().metrics().num_alive_tasks()` to count active tasks. Compare before and after.

**Struct: `ResourceSnapshot`**

Fields:
- `open_ports: usize`
- `tokio_tasks: usize`
- `timestamp: Instant`

Methods:
- `fn take() -> Self` — snapshot current resource state.
- `fn assert_no_leak(&self, after: &ResourceSnapshot)` — verify counts match (within tolerance).

---

## Test Cases

| ID | Test | Verifies |
|---|---|---|
| 72-T1 | `test_loopback_harness_start_stop` | Harness starts receiver, receiver accepts connections, harness stops cleanly |
| 72-T2 | `test_loopback_basic_ap2_stream` | AP2 client streams to receiver, audio verified |
| 72-T3 | `test_loopback_basic_ap1_stream` | AP1 client streams to receiver, audio verified |
| 72-T4 | `test_loopback_auto_protocol` | Client with `Auto` protocol connects to universal receiver |
| 72-T5 | `test_client_factory_ap2` | Factory creates correct AP2 client config |
| 72-T6 | `test_client_factory_ap1` | Factory creates correct AP1 client config |
| 72-T7 | `test_device_construction` | `create_device_for_receiver` produces valid device |
| 72-T8 | `test_matrix_all_combinations` | Matrix runner completes for 2 protocols × 2 codecs |
| 72-T9 | `test_resource_snapshot` | Resource counts match before and after clean test |
| 72-T10 | `test_event_assertion_helpers` | Event helpers correctly detect presence/absence of events |

---

## Acceptance Criteria

- [ ] `LoopbackHarness` starts receiver and creates client in under 2 seconds
- [ ] `run_stream_test` completes a full stream cycle in under 10 seconds
- [ ] Both AP2 and AP1 protocols work through the harness
- [ ] Audio verification passes for all supported codecs
- [ ] No resource leaks detected after test completion
- [ ] Parametric matrix runner covers all protocol × codec combinations
- [ ] Helper functions simplify writing individual tests to <20 lines

---

## References

- `tests/common/receiver_harness.rs` — Section 70 receiver harness
- `src/client/mod.rs` — `AirPlayClient`
- `src/client/protocol.rs` — `PreferredProtocol`, `select_protocol()`
- `src/types/config.rs` — `AirPlayConfig`
- `tests/common/python_receiver.rs` — `TestSineSource`
- `tests/common/audio_verify.rs` — Section 65 audio verification
