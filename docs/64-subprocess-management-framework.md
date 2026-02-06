# Section 64: Subprocess Management Framework

## Dependencies
- **Section 63**: Integration Test Strategy & Roadmap
- `tests/common/python_receiver.rs` — existing pattern to generalise

## Overview

All third-party integration tests require managing external processes (Python receiver, shairport-sync, pyatv driver scripts). This section defines a reusable subprocess framework that handles lifecycle management, port allocation, health checking, log capture, and cleanup. It generalises the patterns already proven in `PythonReceiver::start()`.

## Objectives

- Extract common subprocess logic from `PythonReceiver` into a reusable `SubprocessHandle`
- Provide deterministic port allocation that prevents conflicts across parallel tests
- Capture stdout/stderr into structured logs with timestamps
- Guarantee cleanup even on test panics
- Support both "wait for ready" and "poll for ready" startup patterns

---

## Tasks

### 64.1 SubprocessHandle — Generic Process Wrapper

**File:** `tests/common/subprocess.rs`

**Struct: `SubprocessConfig`**

Fields:
- `command: String` — executable path or name (e.g., `"python3"`, `"shairport-sync"`)
- `args: Vec<String>` — command-line arguments
- `working_dir: Option<PathBuf>` — working directory for the process
- `env_vars: HashMap<String, String>` — extra environment variables
- `ready_pattern: String` — string to search for in stdout/stderr indicating readiness (e.g., `"serving on"`, `"Listening on"`)
- `ready_timeout: Duration` — max wait time for ready pattern (default: 15 seconds)
- `graceful_shutdown_signal: Signal` — signal to send for graceful stop (default: `SIGTERM`)
- `shutdown_timeout: Duration` — max wait after graceful signal before `SIGKILL` (default: 5 seconds)
- `log_prefix: String` — prefix for log lines (e.g., `"[shairport]"`, `"[pyatv]"`)

**Struct: `SubprocessHandle`**

Fields:
- `process: Child` — tokio async child process
- `config: SubprocessConfig`
- `started_at: Instant`
- `log_lines: Arc<Mutex<Vec<TimestampedLogLine>>>`
- `ports: Vec<u16>` — ports reserved for this process

Public methods:
- `async fn spawn(config: SubprocessConfig) -> Result<Self, SubprocessError>` — spawn process, wait for ready pattern, start log drain tasks. Returns error if process exits early, times out, or ready pattern not found.
- `async fn stop(mut self) -> Result<SubprocessOutput, SubprocessError>` — send graceful shutdown signal, wait `shutdown_timeout`, force kill if needed, collect final logs.
- `fn pid(&self) -> Option<u32>` — OS process ID.
- `fn elapsed(&self) -> Duration` — time since spawn.
- `fn logs(&self) -> Vec<TimestampedLogLine>` — snapshot of captured log lines.
- `async fn is_running(&mut self) -> bool` — check if process is still alive.

**Struct: `SubprocessOutput`**

Fields:
- `exit_status: Option<ExitStatus>`
- `logs: Vec<TimestampedLogLine>`
- `duration: Duration`

**Struct: `TimestampedLogLine`**

Fields:
- `timestamp: Instant` — relative to process start
- `stream: LogStream` — `Stdout` or `Stderr`
- `line: String`

**Enum: `SubprocessError`**

Variants:
- `SpawnFailed { command: String, source: io::Error }`
- `ReadyTimeout { timeout: Duration, stderr_tail: Vec<String> }`
- `EarlyExit { status: ExitStatus, stderr_tail: Vec<String> }`
- `ShutdownFailed { source: io::Error }`

**Implementation notes:**
- Pattern the `spawn` method after `PythonReceiver::start()` lines 49–169, but using `SubprocessConfig` fields instead of hardcoded values.
- The background log drain task (lines 150–169 of `python_receiver.rs`) should write to the shared `log_lines` vec instead of just logging via `tracing`. Still emit `tracing::debug!` for real-time visibility.
- The `Drop` impl must call `process.start_kill()` (synchronous kill initiation) since `Drop` cannot be async. Rely on `kill_on_drop(true)` as backup.

**Edge cases:**
- Process that writes ready pattern to stderr instead of stdout (shairport-sync does this) — scan both streams, same as existing `PythonReceiver`.
- Process that outputs ready pattern before all ports are bound — add optional post-ready delay (`SubprocessConfig::post_ready_delay: Option<Duration>`).
- Extremely verbose process flooding logs — add optional `max_log_lines: usize` cap (default: 10000).

---

### 64.2 Port Allocation

**File:** `tests/common/ports.rs`

**Function: `fn reserve_port() -> Result<u16, PortError>`**

Strategy: bind a TCP listener to port 0, read the assigned port, close the listener, return the port. This is the same approach used by `portpicker::pick_unused_port()` but we need a version that can reserve multiple ports atomically.

**Function: `fn reserve_ports(count: usize) -> Result<Vec<u16>, PortError>`**

Bind `count` TCP listeners simultaneously, collect all ports, then drop all listeners. This guarantees no two ports in the batch collide.

**Function: `fn reserve_port_range(count: usize) -> Result<PortRange, PortError>`**

Like `reserve_ports` but attempts to find `count` consecutive ports. Falls back to non-consecutive if consecutive allocation fails after 10 attempts.

**Struct: `PortRange`**

Fields:
- `base: u16`
- `ports: Vec<u16>`

Methods:
- `fn get(&self, index: usize) -> u16`
- `fn iter(&self) -> impl Iterator<Item = u16>`

**Edge cases:**
- Port reuse by OS between reservation and subprocess binding — mitigate by reserving ports as late as possible before spawn. Accept that this is inherently racy but unlikely with dynamic ports.
- Exhaustion on CI runners with many parallel jobs — use port range 10000–60000 to avoid well-known ports.

**Optimisation:** if we need to pass ports to subprocesses via command-line args (e.g., shairport-sync config), reserve them first, generate the config, then spawn.

---

### 64.3 Log Capture & Diagnostics

**File:** `tests/common/diagnostics.rs`

**Function: `fn save_test_logs(test_name: &str, logs: &[TimestampedLogLine], extra_files: &[(&str, &[u8])]) -> PathBuf`**

Writes all log lines to `target/integration-tests/{test_name}/{timestamp}/`. Also copies any extra files (audio dumps, RTP captures) into the same directory.

**Function: `fn format_log_line(line: &TimestampedLogLine) -> String`**

Format: `[+{elapsed_ms}ms] [{stream}] {line}`

**Struct: `TestDiagnostics`**

Collects diagnostic info across a test run:
- `test_name: String`
- `subprocess_logs: HashMap<String, Vec<TimestampedLogLine>>` — keyed by subprocess name
- `audio_files: Vec<(String, Vec<u8>)>`
- `rtp_captures: Vec<(String, Vec<u8>)>`

Method:
- `fn save(&self) -> PathBuf` — writes everything to target directory, returns the directory path.

**Integration with existing pattern:** the existing `ReceiverOutput::log_path` field (`python_receiver.rs:258`) should be migrated to use `TestDiagnostics` so all test suites share the same artifact layout.

---

### 64.4 Refactor PythonReceiver to Use SubprocessHandle

**File:** `tests/common/python_receiver.rs`

Refactor `PythonReceiver` to wrap a `SubprocessHandle` internally rather than managing `Child` directly. This validates the framework against the existing passing tests before using it for shairport-sync and pyatv.

Steps:
1. Replace `process: Child` field with `handle: SubprocessHandle`.
2. Replace `PythonReceiver::start()` body with `SubprocessHandle::spawn(SubprocessConfig { ... })`.
3. Replace `PythonReceiver::stop()` body with `self.handle.stop()` plus audio file reading logic.
4. Keep `device_config()` and `ReceiverOutput` unchanged.
5. Verify all existing integration tests still pass.

**Risk:** this refactor could break existing passing tests. Mitigate by keeping it behind a feature flag or doing it as a separate PR with careful CI validation.

---

### 64.5 Health Check Polling

**File:** `tests/common/subprocess.rs` (addition to `SubprocessHandle`)

Some subprocesses (especially our own receiver in loopback tests) don't have a clear "ready" log message. For these, provide a TCP health check.

**Function: `async fn wait_for_tcp_port(addr: SocketAddr, timeout: Duration) -> Result<(), SubprocessError>`**

Poll-connect to the given address every 100ms until success or timeout. Used as an alternative to `ready_pattern` matching.

**Function: `async fn wait_for_port_bound(port: u16, timeout: Duration) -> Result<(), SubprocessError>`**

Equivalent to `wait_for_tcp_port` on `127.0.0.1:port`.

Add to `SubprocessConfig`:
- `ready_strategy: ReadyStrategy` — enum with `LogPattern(String)`, `TcpPort(u16)`, `Delay(Duration)`, `Custom(Box<dyn Fn() -> Pin<Box<dyn Future<Output = bool>>>>)`

---

## Test Cases

| ID | Test | Verifies |
|---|---|---|
| 64-T1 | `test_subprocess_spawn_and_stop` | Basic lifecycle: spawn a `sleep 30`, verify running, stop, verify exited |
| 64-T2 | `test_subprocess_ready_detection` | Spawn `echo "ready" && sleep 30`, verify ready pattern detected |
| 64-T3 | `test_subprocess_early_exit` | Spawn `exit 1`, verify `EarlyExit` error returned |
| 64-T4 | `test_subprocess_ready_timeout` | Spawn `sleep 30` with 1s ready timeout, verify `ReadyTimeout` error |
| 64-T5 | `test_subprocess_log_capture` | Spawn process that prints 100 lines, verify all captured |
| 64-T6 | `test_subprocess_stderr_ready` | Spawn process that writes ready pattern to stderr, verify detection |
| 64-T7 | `test_reserve_ports_no_collisions` | Reserve 10 ports, verify all distinct |
| 64-T8 | `test_reserve_ports_consecutive` | Reserve 3 consecutive ports, verify `port[n+1] = port[n] + 1` |
| 64-T9 | `test_python_receiver_refactored` | Existing `test_pcm_streaming_end_to_end` still passes with refactored `PythonReceiver` |
| 64-T10 | `test_tcp_health_check` | Start TCP listener, verify `wait_for_tcp_port` succeeds |
| 64-T11 | `test_tcp_health_check_timeout` | No listener, verify timeout after specified duration |

---

## Acceptance Criteria

- [ ] `SubprocessHandle` can manage any of: Python receiver, shairport-sync, pyatv driver
- [ ] All existing `integration_tests.rs` tests pass after `PythonReceiver` refactor
- [ ] Port reservation prevents conflicts across concurrent test runs
- [ ] Logs are captured and available in test output on failure
- [ ] Process cleanup happens even on test panic (via `kill_on_drop`)
- [ ] No zombie processes left after test suite completes

---

## References

- `tests/common/python_receiver.rs` — existing implementation to generalise
- `tests/common/mod.rs` — existing test utilities
- `.github/workflows/integration.yml` — existing CI pipeline
- `portpicker` crate — port allocation approach
