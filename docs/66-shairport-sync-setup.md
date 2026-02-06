# Section 66: shairport-sync Build, Configuration & Subprocess Wrapper

## Dependencies
- **Section 64**: Subprocess Management Framework
- **Section 63**: Integration Test Strategy

## Overview

shairport-sync is a mature C-based AirPlay receiver that supports both AirPlay 1 (RAOP) and AirPlay 2. We use it as a reference receiver to validate our client implementations. This section covers everything needed to build shairport-sync from source, generate test configurations, wrap it as a subprocess, and capture its audio output for verification.

shairport-sync is complex to set up — it has many build options, requires specific system libraries, and its configuration is file-based. Getting it to run on loopback without a real audio device and without conflicting with system mDNS daemons is the primary challenge.

## Objectives

- Build shairport-sync from source with all needed features enabled
- Generate per-test configuration files with unique ports and names
- Wrap as a subprocess using the `SubprocessHandle` from Section 64
- Capture audio output via pipe backend for verification
- Work on CI (Ubuntu) without real audio hardware or network

---

## Tasks

### 66.1 Build Requirements & Version Pinning

**Target version:** shairport-sync 4.3.x (latest stable with AirPlay 2 support)

**Repository:** `https://github.com/mikebrady/shairport-sync`

**Build dependencies (Ubuntu/Debian):**
```
build-essential autoconf automake libtool libpopt-dev libconfig-dev
libssl-dev libsoxr-dev libavahi-client-dev avahi-daemon
libplist-dev libsodium-dev libgcrypt20-dev libpulse-dev
xxd                         # for AirPlay 2 support
libavcodec-dev libavformat-dev libswresample-dev  # for ALAC/AAC decode
libmosquitto-dev            # optional, for MQTT metadata
```

**macOS (Homebrew):**
```
autoconf automake libtool popt libconfig openssl soxr libplist libsodium
```

**Configure flags (minimum for our tests):**
```
./autoreconf -fi
./configure --with-ssl=openssl \
            --with-soxr \
            --with-avahi \
            --with-airplay-2 \
            --with-pipe \
            --with-stdout \
            --with-metadata \
            --with-apple-alac \
            --sysconfdir=/tmp/shairport-sync-test
```

Key flags explained:
- `--with-airplay-2` — enables AirPlay 2 protocol support (requires `libplist`, `libsodium`, `libgcrypt`, `xxd`)
- `--with-pipe` — enables the pipe audio backend (writes raw PCM to a named pipe or file)
- `--with-stdout` — enables stdout backend (alternative capture method)
- `--with-metadata` — enables metadata pipe (track info, volume changes)
- `--with-apple-alac` — enables Apple ALAC decoder

**Version pinning strategy:** clone at a specific tag or commit hash. Store the hash in a constant in the test code:
```
const SHAIRPORT_SYNC_VERSION: &str = "4.3.4";
const SHAIRPORT_SYNC_COMMIT: &str = "abc123...";
```

**Build script location:** `tests/shairport/build.sh`

This script should:
1. Check if shairport-sync binary already exists at `target/shairport-sync/bin/shairport-sync`.
2. If not, clone the repo, checkout the pinned version, configure, and build.
3. Cache the binary in `target/shairport-sync/` to avoid rebuilding on every test run.
4. Exit with clear error messages if any dependency is missing.

**Uncertainties:**
- AirPlay 2 support in shairport-sync may require NQPTP (Network Quality PTP daemon) running separately. Investigation needed to determine if tests can work without it or if we need to also build/run NQPTP.
- Build may fail on some CI runners due to missing `-dev` packages. The CI workflow must install all deps explicitly.
- Apple ALAC decoder may require additional setup. The `--with-apple-alac` flag builds from Apple's open-source ALAC code included in shairport-sync.

---

### 66.2 Configuration Generation

**File:** `tests/common/shairport_sync.rs`

shairport-sync uses a libconfig-format configuration file. We generate a unique config per test run to avoid port conflicts and state contamination.

**Struct: `ShairportConfig`**

Fields:
- `name: String` — service name (e.g., `"test-receiver-{random}"`)
- `port: u16` — RTSP listen port (from port allocator)
- `password: Option<String>` — AirPlay password (None = no password)
- `pipe_path: PathBuf` — named pipe or file for audio output
- `metadata_pipe_path: Option<PathBuf>` — optional metadata pipe
- `output_backend: OutputBackend` — `Pipe` or `Stdout`
- `audio_format: ShairportAudioFormat` — output sample format config
- `airplay2_enabled: bool` — enable AirPlay 2 (may require NQPTP)
- `interface: Option<String>` — bind to specific interface (e.g., `"lo"`)
- `log_verbosity: u8` — 0–3
- `udp_port_base: u16` — base port for RTP/control/timing UDP sockets

**Enum: `OutputBackend`** — `Pipe`, `Stdout`

**Enum: `ShairportAudioFormat`** — `S16LE`, `S24LE`, `S32LE`, `F32LE`

**Method: `fn generate_config_file(&self) -> Result<PathBuf, io::Error>`**

Writes a config file to `target/shairport-sync/configs/{name}.conf` and returns the path. Config template:

```
general = {
    name = "{name}";
    port = {port};
    {password_line}
    output_backend = "{backend}";
    mdns_backend = "avahi";
    interpolation = "basic";
    interface = "{interface}";
};

sessioncontrol = {
    allow_session_interruption = "yes";
};

pipe = {
    name = "{pipe_path}";
    audio_backend_buffer_desired_length_in_seconds = 1.0;
};

metadata = {
    enabled = "{metadata_enabled}";
    {metadata_pipe_line}
};

diagnostics = {
    log_verbosity = {log_verbosity};
};

airplay = {
    udp_port_base = {udp_port_base};
    udp_port_range = 100;
};
```

**Edge cases:**
- Config file path must not contain spaces (libconfig limitation on some versions).
- Pipe path must be created before shairport-sync starts (`mkfifo` for FIFO, or touch for regular file).
- If using FIFO, a reader must be attached before shairport-sync writes, or it will block. Use a background reader task.

---

### 66.3 ShairportSync Subprocess Wrapper

**File:** `tests/common/shairport_sync.rs`

**Struct: `ShairportSync`**

Fields:
- `handle: SubprocessHandle` — from Section 64
- `config: ShairportConfig`
- `config_file_path: PathBuf`
- `audio_capture: Option<JoinHandle<Vec<u8>>>` — background task reading from pipe

Public methods:

**`async fn start(config: ShairportConfig) -> Result<Self, ShairportError>`**

Steps:
1. Generate config file via `config.generate_config_file()`.
2. If using pipe backend, create the named pipe with `mkfifo`.
3. Reserve ports via Section 64 port allocator (RTSP port + UDP base range).
4. Build `SubprocessConfig`:
   - `command`: path to built shairport-sync binary (`target/shairport-sync/bin/shairport-sync`)
   - `args`: `["-c", config_path, "--log-to-stderr"]`
   - `ready_pattern`: `"Listening for service"` or `"shairport-sync starting"` (verify exact string from shairport-sync logs)
   - `ready_timeout`: 15 seconds (shairport-sync can be slow to start with Avahi)
5. Spawn via `SubprocessHandle::spawn(...)`.
6. If using pipe backend, spawn a background task that opens the pipe and reads into a buffer.
7. Return `ShairportSync` instance.

**`async fn stop(self) -> Result<ShairportOutput, ShairportError>`**

Steps:
1. Stop the subprocess via `self.handle.stop()`.
2. If using pipe backend, join the audio capture task to get the audio data.
3. Read metadata pipe if configured.
4. Clean up config file and pipes.
5. Return `ShairportOutput`.

**`fn device_config(&self) -> AirPlayDevice`**

Construct an `AirPlayDevice` for our client to connect to. Set:
- `addresses: vec!["127.0.0.1".parse().unwrap()]`
- `port: self.config.port`
- `capabilities` appropriate for AP1 or AP2 depending on `self.config.airplay2_enabled`
- `raop_port: Some(self.config.port)` — for AP1, RTSP is on this port
- `raop_capabilities`: built from known shairport-sync capabilities (`RaopCapabilities { codecs: vec![Pcm, Alac], encryption_types: vec![Rsa, None], ... }`)

**Struct: `ShairportOutput`**

Fields:
- `audio_data: Option<Vec<u8>>` — raw audio from pipe backend
- `metadata: Option<Vec<u8>>` — raw metadata pipe output
- `logs: Vec<TimestampedLogLine>` — from subprocess
- `exit_status: Option<ExitStatus>`

Methods:
- `fn to_raw_audio(&self) -> Result<RawAudio, AudioError>` — convert captured bytes to `RawAudio` (Section 65) using the configured audio format.
- `fn verify_audio_received(&self) -> Result<(), ShairportError>` — basic non-empty check.

---

### 66.4 Avahi / mDNS Daemon Management

**File:** `tests/common/shairport_sync.rs` (or separate `tests/common/avahi.rs`)

shairport-sync requires an mDNS daemon (Avahi on Linux, mDNSResponder on macOS) to advertise its service. On CI runners, Avahi may not be running.

**Strategy options:**

1. **Start Avahi in test setup** — run `avahi-daemon --no-drop-root --no-chroot` as a subprocess with a custom config that only listens on loopback. This is the most reliable approach.
2. **Use shairport-sync's built-in tinysvcmdns** — if compiled with `--with-tinysvcmdns` instead of `--with-avahi`. This avoids the Avahi dependency entirely but may have limitations.
3. **Skip mDNS entirely** — our client constructs the `AirPlayDevice` manually (hardcoded address and port), bypassing discovery. The client connects directly without needing to discover the service. This works for most tests but doesn't exercise the discovery path.

**Recommended approach:** option 3 (skip mDNS) for most tests, option 1 (Avahi) for discovery-specific tests only.

**Struct: `AvahiDaemon`**

Fields:
- `handle: SubprocessHandle`
- `config_path: PathBuf`

Methods:
- `async fn start_on_loopback() -> Result<Self, AvahiError>` — generate minimal Avahi config binding to loopback, start daemon.
- `async fn stop(self) -> Result<(), AvahiError>`

Avahi loopback config:
```
[server]
host-name=test-airplay
domain-name=local
use-ipv4=yes
use-ipv6=no
allow-interfaces=lo
enable-dbus=no

[publish]
publish-addresses=yes
publish-hinfo=no
publish-workstation=no

[reflector]
enable-reflector=no
```

**Uncertainty:** Avahi may refuse to start on loopback without `--no-drop-root`. CI runners typically run as root in containers, which helps. On non-root environments, may need `sudo` or a different approach.

---

### 66.5 Audio Pipe Reader

**File:** `tests/common/shairport_sync.rs`

When shairport-sync is configured with the pipe backend, it writes raw PCM to a named pipe. We need a background reader.

**Function: `async fn start_pipe_reader(pipe_path: &Path) -> (JoinHandle<Vec<u8>>, oneshot::Sender<()>)`**

Spawns a tokio task that:
1. Opens the named pipe for reading (this blocks until the writer opens it).
2. Reads in 4096-byte chunks into a `Vec<u8>`.
3. Stops when the `oneshot` signal is received or EOF is reached.
4. Returns the accumulated audio data.

**Edge cases:**
- Pipe must exist before shairport-sync starts (created in `ShairportSync::start()`).
- If shairport-sync never writes (no client connects), the reader blocks on open. Use a timeout or `tokio::select!` with a cancellation channel.
- Large audio streams could consume excessive memory — cap buffer at e.g., 100 MB and discard oldest data.
- On macOS, named pipes behave differently. Test with both `mkfifo` and regular file fallback.

---

### 66.6 Build Caching & CI Integration

**File:** `tests/shairport/build.sh`

The build script must be idempotent and fast on cache hit.

Steps:
1. Check `target/shairport-sync/bin/shairport-sync` exists and is executable.
2. If yes, check version matches (`shairport-sync --version` output contains expected version string).
3. If cache valid, exit 0.
4. If cache invalid or missing:
   a. `rm -rf target/shairport-sync/src`
   b. `git clone --depth 1 --branch {version} https://github.com/mikebrady/shairport-sync target/shairport-sync/src`
   c. `cd target/shairport-sync/src && autoreconf -fi && ./configure {flags} --prefix=target/shairport-sync && make -j$(nproc) && make install`
5. Verify binary works: `target/shairport-sync/bin/shairport-sync --version`

**CI caching:** use GitHub Actions cache with key `shairport-sync-{version}-{os}-{hash-of-build.sh}`.

**Uncertainty:** shairport-sync build time is 1-3 minutes with all features. With caching, subsequent runs are instant. First run on a new CI cache will be slow.

---

## Test Cases

| ID | Test | Verifies |
|---|---|---|
| 66-T1 | `test_shairport_binary_exists` | Build script ran, binary at expected path |
| 66-T2 | `test_shairport_version_matches` | Binary reports expected version |
| 66-T3 | `test_config_generation_basic` | Config file generated with correct name, port, pipe path |
| 66-T4 | `test_config_generation_with_password` | Config includes password line when set |
| 66-T5 | `test_config_generation_ap2_enabled` | Config enables AirPlay 2 features |
| 66-T6 | `test_shairport_start_stop` | Start shairport-sync, verify ready, stop, verify exited |
| 66-T7 | `test_shairport_device_config` | `device_config()` returns valid `AirPlayDevice` |
| 66-T8 | `test_shairport_pipe_creation` | Named pipe created before process start |
| 66-T9 | `test_pipe_reader_receives_data` | Write known data to pipe, verify reader captures it |
| 66-T10 | `test_shairport_logs_captured` | Verify log lines captured from stderr |
| 66-T11 | `test_shairport_port_allocation` | Reserved ports match config file values |
| 66-T12 | `test_shairport_cleanup_on_error` | If start fails, config files and pipes are cleaned up |

---

## Acceptance Criteria

- [ ] shairport-sync builds from source on Ubuntu with a single script
- [ ] Build is cached and reused across test runs
- [ ] Config generation produces valid shairport-sync configs for AP1 and AP2
- [ ] `ShairportSync::start()` spawns the process and detects readiness
- [ ] Audio captured via pipe backend matches expected format
- [ ] `device_config()` produces an `AirPlayDevice` our client can connect to
- [ ] Cleanup happens on both success and failure paths

---

## References

- [shairport-sync GitHub](https://github.com/mikebrady/shairport-sync)
- [shairport-sync CONFIGURATION](https://github.com/mikebrady/shairport-sync/blob/master/scripts/shairport-sync.conf)
- [shairport-sync BUILD](https://github.com/mikebrady/shairport-sync/blob/master/INSTALL.md)
- [NQPTP](https://github.com/mikebrady/nqptp) — PTP daemon required for AirPlay 2
- `tests/common/subprocess.rs` — Section 64 subprocess framework
