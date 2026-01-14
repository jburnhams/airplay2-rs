# Section 01: Project Setup & CI/CD

## Dependencies
- None (this is the foundation section)

## Overview

This section establishes the project structure, build configuration, CI/CD pipeline, and development tooling. All other sections depend on this being completed first.

## Objectives

- Initialize Cargo workspace with proper structure
- Configure linting, formatting, and code quality tools
- Set up GitHub Actions for CI/CD
- Establish testing infrastructure
- Configure feature flags

---

## Tasks

### 1.1 Project Initialization

- [x] **1.1.1** Create Cargo.toml with proper metadata

**File:** `Cargo.toml`

```toml
[package]
name = "airplay2"
version = "0.1.0"
edition = "2024"
rust-version = "1.83"
authors = ["Your Name <email@example.com>"]
description = "Pure Rust library for streaming audio to AirPlay 2 devices"
license = "MIT OR Apache-2.0"
repository = "https://github.com/org/airplay2-rs"
keywords = ["airplay", "audio", "streaming", "homekit", "multiroom"]
categories = ["multimedia::audio", "network-programming"]
readme = "README.md"

[features]
default = ["tokio-runtime"]
tokio-runtime = ["tokio", "tokio-util"]
async-std-runtime = ["async-std"]
persistent-pairing = ["sled"]

[dependencies]
# Async
tokio = { version = "1.43", features = ["net", "sync", "time", "rt", "macros"], optional = true }
tokio-util = { version = "0.7", features = ["codec"], optional = true }
async-std = { version = "1.13", optional = true }
async-trait = "0.1"
futures = "0.3"

# Error handling
thiserror = "2.0"

# Logging
tracing = "0.1"

# Discovery
mdns-sd = "0.12"

# Crypto
srp = "0.7"
x25519-dalek = { version = "2.0", features = ["static_secrets"] }
ed25519-dalek = { version = "2.1", features = ["rand_core"] }
chacha20poly1305 = "0.10"
aes-gcm = "0.10"
aes = "0.8"
ctr = "0.9"
hkdf = "0.12"
sha2 = "0.10"
rand = "0.8"
curve25519-dalek = "4.1"

# Serialization
bytes = "1.9"

# Audio processing
rubato = "0.14"           # Audio resampling

# Persistent storage (optional)
sled = { version = "0.34", optional = true }

[dev-dependencies]
tokio = { version = "1.43", features = ["full", "test-util"] }
tokio-test = "0.4"
proptest = "1.6"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
criterion = { version = "0.5", features = ["async_tokio"] }
tempfile = "3.15"

[[bench]]
name = "protocol_benchmarks"
harness = false
```

- [x] **1.1.2** Create initial `src/lib.rs` with module structure stubs

**File:** `src/lib.rs`

```rust
//! # airplay2
//!
//! A pure Rust library for streaming audio to AirPlay 2 devices.
//!
//! ## Features
//!
//! - Device discovery via mDNS
//! - HomeKit authentication
//! - Audio streaming (PCM and URL-based)
//! - Playback control
//! - Multi-room synchronized playback
//!
//! ## Example
//!
//! ```rust,no_run
//! use airplay2::{discover, AirPlayClient};
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), airplay2::AirPlayError> {
//! // Discover devices
//! let devices = airplay2::scan(Duration::from_secs(5)).await?;
//!
//! if let Some(device) = devices.first() {
//!     // Connect to device
//!     let client = AirPlayClient::connect(device).await?;
//!
//!     // Stream audio...
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Public modules
pub mod types;
pub mod error;

// Internal modules
mod protocol;
mod discovery;
mod net;
mod connection;
mod audio;
mod control;
mod group;
mod client;
mod player;

// Re-exports
pub use error::AirPlayError;
pub use types::{
    AirPlayDevice, AirPlayConfig, TrackInfo,
    PlaybackState, RepeatMode, PlaybackInfo,
};
pub use client::AirPlayClient;
pub use player::AirPlayPlayer;
pub use group::AirPlayGroup;

// Discovery functions
pub use discovery::{discover, scan};
```

- [x] **1.1.3** Create placeholder modules for all components

**Files to create (empty mod.rs or stub files):**
- `src/types/mod.rs`
- `src/error.rs`
- `src/protocol/mod.rs`
- `src/discovery/mod.rs`
- `src/net/mod.rs`
- `src/connection/mod.rs`
- `src/audio/mod.rs`
- `src/control/mod.rs`
- `src/group/mod.rs`
- `src/client.rs`
- `src/player.rs`

Each stub should contain minimal code to allow compilation:

```rust
//! Module description here

// TODO: Implement in Section XX
```

---

### 1.2 Development Tooling

- [x] **1.2.1** Create `rustfmt.toml` for consistent formatting

**File:** `rustfmt.toml`

```toml
edition = "2024"
max_width = 100
use_small_heuristics = "Default"
imports_granularity = "Module"
group_imports = "StdExternalCrate"
reorder_imports = true
reorder_modules = true
newline_style = "Unix"
use_field_init_shorthand = true
use_try_shorthand = true
format_code_in_doc_comments = true
format_macro_matchers = true
format_strings = true
wrap_comments = true
comment_width = 100
normalize_comments = true
```

- [x] **1.2.2** Create `clippy.toml` for linting configuration

**File:** `clippy.toml`

```toml
cognitive-complexity-threshold = 25
too-many-arguments-threshold = 8
type-complexity-threshold = 300
```

- [x] **1.2.3** Create `.cargo/config.toml` for build configuration

**File:** `.cargo/config.toml`

```toml
[alias]
xtask = "run --package xtask --"

[build]
rustflags = ["-D", "warnings"]

[target.x86_64-unknown-linux-gnu]
rustflags = ["-D", "warnings"]

[target.x86_64-apple-darwin]
rustflags = ["-D", "warnings"]

[target.aarch64-apple-darwin]
rustflags = ["-D", "warnings"]

[target.x86_64-pc-windows-msvc]
rustflags = ["-D", "warnings"]
```

- [x] **1.2.4** Create `deny.toml` for dependency auditing

**File:** `deny.toml`

```toml
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
notice = "warn"

[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "CC0-1.0",
    "Unicode-DFS-2016",
]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "all"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

---

### 1.3 GitHub Actions CI/CD

- [x] **1.3.1** Create main CI workflow

**File:** `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-features

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-features --all-targets -- -D warnings

  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features

  doc:
    name: Documentation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo doc --no-deps --all-features
        env:
          RUSTDOCFLAGS: -D warnings

  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --all-features --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: false

  deny:
    name: Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v1
```

- [x] **1.3.2** Create security audit workflow

**File:** `.github/workflows/security.yml`

```yaml
name: Security Audit

on:
  schedule:
    - cron: '0 0 * * *'
  push:
    paths:
      - '**/Cargo.toml'
      - '**/Cargo.lock'

jobs:
  audit:
    name: Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

- [x] **1.3.3** Create release workflow

**File:** `.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    name: Publish to crates.io
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

---

### 1.4 Project Documentation

- [x] **1.4.1** Create README.md

**File:** `README.md`

```markdown
# airplay2-rs

A pure Rust library for streaming audio to AirPlay 2 devices.

## Features

- **Device Discovery**: Find AirPlay 2 devices on your network via mDNS
- **HomeKit Authentication**: Secure pairing with Apple devices
- **Audio Streaming**: Stream PCM audio or URLs to devices
- **Playback Control**: Play, pause, seek, volume, and queue management
- **Multi-room Audio**: Synchronized playback across multiple devices

## Installation

Add to your `Cargo.toml`:

\`\`\`toml
[dependencies]
airplay2 = "0.1"
\`\`\`

## Quick Start

\`\`\`rust
use airplay2::{scan, AirPlayClient, TrackInfo};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), airplay2::AirPlayError> {
    // Discover devices on the network
    let devices = scan(Duration::from_secs(5)).await?;

    println!("Found {} devices", devices.len());

    if let Some(device) = devices.first() {
        println!("Connecting to: {}", device.name);

        let mut client = AirPlayClient::connect(device).await?;

        // Load a track
        let track = TrackInfo {
            url: "http://example.com/audio.mp3".to_string(),
            title: "Example Track".to_string(),
            artist: "Artist".to_string(),
            ..Default::default()
        };

        client.load(&track).await?;
        client.play().await?;
    }

    Ok(())
}
\`\`\`

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
```

- [x] **1.4.2** Create CONTRIBUTING.md

- [x] **1.4.3** Create LICENSE-MIT and LICENSE-APACHE files

---

### 1.5 Test Infrastructure

- [x] **1.5.1** Create test utilities module

**File:** `tests/common/mod.rs`

```rust
//! Common test utilities and fixtures

use std::sync::Once;
use tracing_subscriber::{fmt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize test logging (call once per test module)
pub fn init_logging() {
    INIT.call_once(|| {
        let filter = EnvFilter::from_default_env()
            .add_directive("airplay2=debug".parse().unwrap());

        fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .init();
    });
}

/// Create a test configuration with short timeouts
pub fn test_config() -> airplay2::AirPlayConfig {
    airplay2::AirPlayConfig {
        discovery_timeout: std::time::Duration::from_millis(100),
        connection_timeout: std::time::Duration::from_millis(500),
        state_poll_interval: std::time::Duration::from_millis(50),
        debug_protocol: true,
    }
}
```

- [x] **1.5.2** Create benchmark harness

**File:** `benches/protocol_benchmarks.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn rtsp_encoding_benchmark(c: &mut Criterion) {
    // TODO: Add benchmarks when RTSP codec is implemented
    c.bench_function("rtsp_encode_request", |b| {
        b.iter(|| {
            // Benchmark code here
        })
    });
}

criterion_group!(benches, rtsp_encoding_benchmark);
criterion_main!(benches);
```

- [x] **1.5.3** Set up example programs structure

**Files:**
- `examples/discover.rs` - Device discovery example
- `examples/play_url.rs` - URL playback example
- `examples/play_pcm.rs` - PCM streaming example
- `examples/multi_room.rs` - Multi-room example

Each example should have a minimal stub that compiles but notes TODO.

---

### 1.6 Git Configuration

- [x] **1.6.1** Create `.gitignore`

**File:** `.gitignore`

```
/target
*.swp
*.swo
.idea/
.vscode/
*.log
.env
.DS_Store
coverage/
*.profraw
```

- [x] **1.6.2** Create `.gitattributes`

**File:** `.gitattributes`

```
* text=auto eol=lf
*.rs text eol=lf
*.toml text eol=lf
*.md text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
```

---

## Unit Tests

### Test: Project compiles with all feature combinations

```rust
#[test]
fn test_default_features_compile() {
    // cargo build (default features)
}

#[test]
fn test_tokio_runtime_compile() {
    // cargo build --features tokio-runtime
}

#[test]
fn test_no_default_features_compile() {
    // cargo build --no-default-features
}
```

### Test: All modules are accessible

```rust
#[test]
fn test_public_api_accessible() {
    use airplay2::{
        AirPlayDevice, AirPlayClient, AirPlayConfig,
        TrackInfo, PlaybackState, AirPlayError,
    };
}
```

---

## Integration Tests

### Test: CI pipeline passes locally

```bash
# Run full CI check locally
cargo fmt --check
cargo clippy --all-features --all-targets
cargo test --all-features
cargo doc --no-deps
```

---

## Acceptance Criteria

- [x] `cargo build` succeeds with default features
- [x] `cargo build --all-features` succeeds
- [x] `cargo build --no-default-features` succeeds
- [x] `cargo test` runs (even if tests are TODO)
- [x] `cargo clippy` passes with no warnings
- [x] `cargo fmt --check` passes
- [x] `cargo doc` generates documentation
- [x] GitHub Actions CI workflow passes
- [x] All placeholder modules exist and compile
- [x] README has basic usage example

---

## Notes

- The Cargo.toml dependency versions should be verified against crates.io at implementation time
- MSRV (1.83) should be tested in CI
- Consider adding a `CHANGELOG.md` with keepachangelog format
