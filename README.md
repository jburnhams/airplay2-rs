# airplay2-rs

A pure Rust library for streaming audio to AirPlay 2 devices.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

## Features

- **Device Discovery**: Find AirPlay 2 devices on your network via mDNS
- **HomeKit Authentication**: Secure pairing with Apple devices (transient and persistent)
- **Audio Streaming**:
  - Stream raw PCM audio (realtime)
  - Stream from URLs (HTTP/HTTPS)
- **Playback Control**: Play, pause, seek, volume, and queue management
- **Multi-room Audio**: Synchronized playback across multiple devices
- **Metadata**: Send track info, album art, and progress updates

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
airplay2 = "0.1"
```

## Quick Start

```rust
use airplay2::{AirPlayPlayer, quick_connect};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), airplay2::AirPlayError> {
    // Quick connect to the first available device
    let player = quick_connect().await?;
    println!("Connected to: {}", player.device().await.unwrap().name);

    // Play a track from URL
    player.play_track(
        "Never Gonna Give You Up",
        "Rick Astley"
    ).await?;

    // Set volume
    player.set_volume(0.5).await?;

    // Wait for a bit
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Disconnect
    player.disconnect().await?;
    Ok(())
}
```

## Documentation

The project documentation is organized into detailed sections:

### Overview
- [00-overview.md](docs/00-overview.md): Project overview and architecture
- [01-project-setup.md](docs/01-project-setup.md): Project setup, dependencies, and CI/CD

### Core Components
- [02-core-types.md](docs/02-core-types.md): Core types, errors, and configuration
- [09-async-runtime.md](docs/09-async-runtime.md): Async runtime abstraction
- [10-connection-management.md](docs/10-connection-management.md): Connection lifecycle and state machine

### Protocols
- [03-binary-plist.md](docs/03-binary-plist.md): Binary property list (bplist) codec
- [04-crypto-primitives.md](docs/04-crypto-primitives.md): Cryptographic primitives (SRP, Ed25519, ChaCha20)
- [05-rtsp-protocol.md](docs/05-rtsp-protocol.md): RTSP protocol (sans-IO)
- [06-rtp-protocol.md](docs/06-rtp-protocol.md): RTP/RAOP protocol (audio transport)
- [07-homekit-pairing.md](docs/07-homekit-pairing.md): HomeKit pairing and authentication
- [08-mdns-discovery.md](docs/08-mdns-discovery.md): mDNS device discovery

### Audio & Streaming
- [11-audio-formats.md](docs/11-audio-formats.md): Audio formats and conversion
- [12-audio-buffer.md](docs/12-audio-buffer.md): Buffering, jitter correction, and timing
- [13-pcm-streaming.md](docs/13-pcm-streaming.md): PCM audio streaming pipeline
- [14-url-streaming.md](docs/14-url-streaming.md): URL-based streaming

### Control & Logic
- [15-playback-control.md](docs/15-playback-control.md): Playback control (play, pause, seek)
- [16-queue-management.md](docs/16-queue-management.md): Queue management
- [17-state-events.md](docs/17-state-events.md): State management and event bus
- [18-volume-control.md](docs/18-volume-control.md): Volume control
- [19-multiroom.md](docs/19-multiroom.md): Multi-room grouping and sync

### API & Testing
- [20-mock-server.md](docs/20-mock-server.md): Mock AirPlay server for testing
- [21-airplay-client.md](docs/21-airplay-client.md): Main `AirPlayClient` implementation
- [22-high-level-api.md](docs/22-high-level-api.md): High-level `AirPlayPlayer` API
- [23-examples.md](docs/23-examples.md): Usage examples

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
