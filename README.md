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

```toml
[dependencies]
airplay2 = "0.1"
```

## Quick Start

```rust
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
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
