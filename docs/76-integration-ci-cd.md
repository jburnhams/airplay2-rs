# Section 76: Integration Test CI/CD Pipeline

## Dependencies
- **Section 63**: Integration Test Strategy
- **Section 66**: shairport-sync Setup
- **Section 69**: pyatv Setup
- **Section 72**: Loopback Test Infrastructure
- `.github/workflows/integration.yml` — existing CI pipeline

## Overview

This section defines the CI/CD infrastructure to run all integration test suites automatically. There are four distinct test suites with different dependency requirements, so they run as separate GitHub Actions workflows (or separate jobs within a workflow) to allow independent caching, failure isolation, and parallelism.

---

## Tasks

### 76.1 Workflow Architecture

**Four workflow files:**

| Workflow File | Test Suite | Dependencies | Run Time |
|---|---|---|---|
| `integration.yml` (existing) | Python receiver + AP2 client | Python 3.11, PyAV, ap2-receiver | ~5 min |
| `integration-shairport.yml` (new) | AP1/AP2 client vs shairport-sync | shairport-sync, NQPTP, Avahi | ~10 min |
| `integration-pyatv.yml` (new) | AP1/AP2 receiver vs pyatv | pyatv, our receiver | ~5 min |
| `integration-loopback.yml` (new) | All loopback + cross-protocol tests | Rust only (no external tools) | ~3 min |

**Triggers:**
```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:
```

**Matrix strategy:** all workflows run on `ubuntu-latest`. macOS matrix entry for loopback tests only (no shairport-sync build complexity on macOS).

---

### 76.2 shairport-sync CI Workflow

**File:** `.github/workflows/integration-shairport.yml`

This is the most complex workflow due to shairport-sync's build dependencies.

**Steps:**

1. **Checkout code** — `actions/checkout@v4`

2. **Set up Rust** — `dtolnay/rust-toolchain@stable`

3. **Cache Rust dependencies** — `Swatinem/rust-cache@v2`

4. **Install shairport-sync build dependencies:**
```yaml
- name: Install shairport-sync build dependencies
  run: |
    sudo apt-get update
    sudo apt-get install -y \
      build-essential autoconf automake libtool \
      libpopt-dev libconfig-dev libssl-dev libsoxr-dev \
      libavahi-client-dev avahi-daemon \
      libplist-dev libsodium-dev libgcrypt20-dev \
      xxd libavcodec-dev libavformat-dev libswresample-dev
```

5. **Build/cache shairport-sync:**
```yaml
- name: Cache shairport-sync binary
  uses: actions/cache@v4
  id: shairport-cache
  with:
    path: target/shairport-sync
    key: shairport-sync-${{ env.SHAIRPORT_VERSION }}-${{ runner.os }}-${{ hashFiles('tests/shairport/build.sh') }}

- name: Build shairport-sync
  if: steps.shairport-cache.outputs.cache-hit != 'true'
  run: bash tests/shairport/build.sh
```

6. **Build/cache NQPTP (for AP2 tests):**
```yaml
- name: Cache NQPTP binary
  uses: actions/cache@v4
  id: nqptp-cache
  with:
    path: target/nqptp
    key: nqptp-${{ env.NQPTP_VERSION }}-${{ runner.os }}

- name: Build NQPTP
  if: steps.nqptp-cache.outputs.cache-hit != 'true'
  run: bash tests/shairport/build_nqptp.sh
```

7. **Start Avahi daemon (for discovery tests):**
```yaml
- name: Start Avahi on loopback
  run: |
    sudo mkdir -p /etc/avahi
    sudo cp tests/shairport/avahi-test.conf /etc/avahi/avahi-daemon.conf
    sudo avahi-daemon --no-drop-root --daemonize
    sleep 2
    avahi-browse -a -t  # Verify Avahi is running
```

8. **Build Rust tests:**
```yaml
- name: Build integration tests
  run: cargo build --release --all-features --tests
```

9. **Run AP1 client tests:**
```yaml
- name: Run AP1 client vs shairport-sync tests
  run: |
    cargo test --test shairport_ap1_client_tests --release -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
    AIRPLAY_TEST_INTERFACE: lo
    SHAIRPORT_BINARY: target/shairport-sync/bin/shairport-sync
  timeout-minutes: 10
```

10. **Run AP2 client tests (requires NQPTP):**
```yaml
- name: Start NQPTP for AP2 tests
  run: |
    sudo target/nqptp/bin/nqptp &
    sleep 1

- name: Run AP2 client vs shairport-sync tests
  run: |
    cargo test --test shairport_ap2_client_tests --release -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
    AIRPLAY_TEST_INTERFACE: lo
    SHAIRPORT_BINARY: target/shairport-sync/bin/shairport-sync
    NQPTP_RUNNING: "1"
  timeout-minutes: 10
```

11. **Upload artifacts on failure:**
```yaml
- name: Upload test artifacts on failure
  if: failure()
  uses: actions/upload-artifact@v4
  with:
    name: shairport-test-logs-${{ matrix.os }}
    path: |
      target/integration-tests/
      target/shairport-sync/configs/
    retention-days: 7
```

**Environment variables used by tests:**
- `SHAIRPORT_BINARY` — path to shairport-sync binary
- `NQPTP_RUNNING` — "1" if NQPTP is available (AP2 tests check this)
- `AIRPLAY_TEST_INTERFACE` — loopback interface name

---

### 76.3 pyatv CI Workflow

**File:** `.github/workflows/integration-pyatv.yml`

**Steps:**

1. **Checkout, Rust, Cache** — same as above.

2. **Set up Python 3.11:**
```yaml
- name: Set up Python
  uses: actions/setup-python@v5
  with:
    python-version: '3.11'
    cache: 'pip'
    cache-dependency-path: tests/pyatv/requirements.txt
```

3. **Install pyatv:**
```yaml
- name: Install pyatv
  run: pip install -r tests/pyatv/requirements.txt
```

4. **Generate test audio files:**
```yaml
- name: Generate test audio files
  run: python3 tests/pyatv/generate_test_audio.py
```

5. **Verify pyatv installation:**
```yaml
- name: Verify pyatv
  run: |
    python3 -c "import pyatv; print(f'pyatv {pyatv.__version__} OK')"
    python3 tests/pyatv/driver_ap2.py --help
    python3 tests/pyatv/driver_ap1.py --help
```

6. **Run AP2 receiver tests:**
```yaml
- name: Run AP2 receiver vs pyatv tests
  run: |
    cargo test --test pyatv_ap2_receiver_tests --release --features receiver -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
    AIRPLAY_TEST_INTERFACE: lo
  timeout-minutes: 10
```

7. **Run AP1 receiver tests:**
```yaml
- name: Run AP1 receiver vs pyatv tests
  run: |
    cargo test --test pyatv_ap1_receiver_tests --release --features receiver -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
    AIRPLAY_TEST_INTERFACE: lo
  timeout-minutes: 10
```

8. **Upload artifacts on failure** — same pattern.

---

### 76.4 Loopback CI Workflow

**File:** `.github/workflows/integration-loopback.yml`

The simplest workflow — no external dependencies, just Rust.

**Steps:**

1. **Checkout, Rust, Cache.**

2. **Build:**
```yaml
- name: Build
  run: cargo build --release --all-features --tests
```

3. **Run AP2 loopback tests:**
```yaml
- name: Run AP2 loopback tests
  run: cargo test --test loopback_ap2_tests --release --features receiver -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
  timeout-minutes: 5
```

4. **Run AP1 loopback tests:**
```yaml
- name: Run AP1 loopback tests
  run: cargo test --test loopback_ap1_tests --release --features receiver -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
  timeout-minutes: 5
```

5. **Run cross-protocol tests:**
```yaml
- name: Run cross-protocol tests
  run: cargo test --test loopback_cross_tests --release --features receiver -- --ignored --test-threads=1 --nocapture
  env:
    RUST_LOG: info
  timeout-minutes: 5
```

6. **Upload artifacts on failure.**

**Matrix:** run on both `ubuntu-latest` and `macos-latest` since loopback tests have no Linux-specific dependencies.

---

### 76.5 Docker Image for shairport-sync (Optional)

**File:** `docker/shairport-sync/Dockerfile`

Alternative to building shairport-sync on every CI run. Pre-built Docker image with shairport-sync, NQPTP, and Avahi.

```dockerfile
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    build-essential autoconf automake libtool \
    libpopt-dev libconfig-dev libssl-dev libsoxr-dev \
    avahi-daemon libavahi-client-dev \
    libplist-dev libsodium-dev libgcrypt20-dev \
    xxd libavcodec-dev libavformat-dev libswresample-dev \
    git

# Build NQPTP
RUN git clone --depth 1 --branch {version} https://github.com/mikebrady/nqptp /tmp/nqptp && \
    cd /tmp/nqptp && autoreconf -fi && ./configure && make && make install

# Build shairport-sync
RUN git clone --depth 1 --branch {version} https://github.com/mikebrady/shairport-sync /tmp/sps && \
    cd /tmp/sps && autoreconf -fi && \
    ./configure --with-ssl=openssl --with-soxr --with-avahi --with-airplay-2 \
                --with-pipe --with-stdout --with-metadata --with-apple-alac && \
    make && make install

# Cleanup
RUN rm -rf /tmp/nqptp /tmp/sps && apt-get clean

COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

**Usage in CI:**
```yaml
services:
  shairport-sync:
    image: ghcr.io/{org}/airplay2-rs-shairport:latest
    options: --privileged  # for NQPTP raw sockets
```

**Trade-off:** Docker adds complexity but eliminates build time on cache miss (~3 min). Worth it if shairport-sync build breaks frequently on CI updates.

---

### 76.6 Test Artifact Management

**Artifacts collected on failure:**

| Artifact | Source | Purpose |
|---|---|---|
| `target/integration-tests/*/` | `TestDiagnostics::save()` | Per-test logs, audio dumps, configs |
| `*.raw` audio files | Audio file sinks | Raw audio for offline analysis |
| `rtp_packets.bin` | RTP capture | Protocol debugging |
| shairport-sync configs | `target/shairport-sync/configs/` | Reproduce exact test config |
| shairport-sync logs | `SubprocessHandle::logs()` | External tool diagnostics |
| pyatv driver JSON output | `tests/pyatv/output/` | Driver script results |

**Retention:** 7 days (same as existing workflow).

**Artifact naming:** `{workflow}-{os}-{timestamp}` to avoid collisions.

---

### 76.7 Test Reporting

**File:** `.github/workflows/integration-report.yml`

Optional: aggregate test results from all four workflows into a single report.

**Approach:** use `dorny/test-reporter@v1` action which parses JUnit XML test output.

**Steps:**
1. Each workflow produces JUnit XML: `cargo test ... -- -Z unstable-options --format json | cargo2junit > results.xml`
   - Note: `cargo2junit` is a third-party tool. Alternative: use `cargo-nextest` which produces JUnit natively.
2. Upload XML as artifact.
3. Reporting workflow downloads all XMLs and produces a combined report.

**Alternative:** use `cargo-nextest` directly:
```yaml
- name: Install nextest
  uses: taiki-e/install-action@nextest

- name: Run tests
  run: cargo nextest run --test loopback_ap2_tests --release --features receiver -- --ignored
  env:
    RUST_LOG: info
```

`cargo-nextest` provides better output, parallel execution control, and JUnit output natively.

---

### 76.8 Scheduled Full Test Runs

Third-party tools may break due to upstream changes (new pyatv version, new shairport-sync version). Run the full suite on a schedule to catch these early.

```yaml
on:
  schedule:
    - cron: '0 4 * * 1'  # Every Monday at 4am UTC
  workflow_dispatch:
```

Only the shairport-sync and pyatv workflows need scheduled runs. Loopback tests only depend on our own code and run on every PR.

---

## Test Cases

| ID | Test | Verifies |
|---|---|---|
| 76-T1 | Manual: trigger `integration-loopback.yml` | Workflow completes, all loopback tests pass |
| 76-T2 | Manual: trigger `integration-shairport.yml` | shairport-sync builds, AP1 tests pass |
| 76-T3 | Manual: trigger `integration-pyatv.yml` | pyatv installs, receiver tests pass |
| 76-T4 | Push to PR branch | All workflows triggered automatically |
| 76-T5 | Intentionally fail a test | Artifacts uploaded, diagnostic output available |
| 76-T6 | Second run with cache | shairport-sync build skipped (cache hit) |

---

## Acceptance Criteria

- [ ] All four workflows run successfully in CI
- [ ] shairport-sync build is cached (cache hit time < 30s)
- [ ] pyatv installation is cached via pip cache
- [ ] Test failures produce downloadable diagnostic artifacts
- [ ] Workflows complete within timeout (10 min each)
- [ ] Loopback tests run on both Ubuntu and macOS
- [ ] No workflow depends on secrets or privileged access (except NQPTP, which needs `--privileged` Docker or `sudo`)
- [ ] Weekly scheduled run catches upstream dependency breakage

---

## References

- `.github/workflows/integration.yml` — existing CI pipeline (model)
- `.github/workflows/ci.yml` — standard CI pipeline
- [GitHub Actions caching](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows)
- [cargo-nextest](https://nexte.st/) — better test runner
- [dorny/test-reporter](https://github.com/dorny/test-reporter) — JUnit report action
