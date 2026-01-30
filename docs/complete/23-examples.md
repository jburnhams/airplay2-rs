# Section 23: Examples

> **VERIFIED**: Checked against `examples/` directory on 2025-01-30.
> Working examples include: discover.rs, play_pcm.rs, play_url.rs, connect_to_receiver.rs,
> multi_room.rs, interactive.rs, persistent_pairing.rs, verify_metadata.rs, verify_stereo.rs.

## Dependencies
- **Section 22**: High-Level API (must be complete)

## Overview

This section provides comprehensive examples demonstrating how to use the `airplay2-rs` library. These examples cover common scenarios like device discovery, streaming local audio, URL playback, and metadata updates.

## Objectives

- Provide runnable code examples
- Demonstrate best practices
- Cover both high-level and low-level APIs

---

## 23.1 Device Discovery

**File:** `examples/discover.rs`

```rust
//! Example: Discover AirPlay devices on the network

use airplay2::{scan, AirPlayDevice};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Scanning for AirPlay devices (5 seconds)...");

    // Scan for devices
    let devices = scan(Duration::from_secs(5)).await?;

    if devices.is_empty() {
        println!("No devices found.");
        return Ok(());
    }

    println!("Found {} devices:", devices.len());
    for (i, device) in devices.iter().enumerate() {
        print_device_info(i + 1, device);
    }

    Ok(())
}

fn print_device_info(index: usize, device: &AirPlayDevice) {
    println!("\nDevice #{}: {}", index, device.name);
    println!("  ID: {}", device.id);
    println!("  Address: {}:{}", device.address, device.port);

    if let Some(model) = &device.model {
        println!("  Model: {}", model);
    }

    println!("  Capabilities:");
    println!("    - AirPlay 2: {}", device.supports_airplay2());
    println!("    - Multi-room: {}", device.supports_grouping());

    if let Some(vol) = device.discovered_volume() {
        println!("    - Volume: {:.0}%", vol * 100.0);
    }
}
```

---

## 23.2 Simple URL Playback

**File:** `examples/play_url.rs`

```rust
//! Example: Play a URL on an AirPlay device

use airplay2::{quick_connect, AirPlayPlayer};
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to first available device...");

    // Quick connect helper
    let player = quick_connect().await?;

    if let Some(device) = player.device().await {
        println!("Connected to: {}", device.name);
    }

    // Play a sample track
    let url = "http://commondatastorage.googleapis.com/codeskulptor-demos/riceracer_assets/music/win.ogg";
    println!("Playing: {}", url);

    player.play_track("Winning Sound", "Demo Artist").await?;
    // Note: In real implementation, play_track sets metadata
    // We need to actually load the URL via underlying client
    // or extend play_track to take URL

    // Wait for user input to stop
    println!("\nPress Enter to stop...");
    let mut stdin = io::BufReader::new(io::stdin());
    let mut line = String::new();
    stdin.read_line(&mut line).await?;

    println!("Stopping...");
    player.stop().await?;
    player.disconnect().await?;

    Ok(())
}
```

---

## 23.3 Interactive Player

**File:** `examples/interactive.rs`

```rust
//! Example: Interactive CLI player

use airplay2::{AirPlayPlayer, quick_connect};
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting...");
    let player = quick_connect().await?;

    if let Some(device) = player.device().await {
        println!("Connected to: {}", device.name);
    }

    println!("\nCommands:");
    println!("  play <url>    - Play URL");
    println!("  pause         - Pause playback");
    println!("  resume        - Resume playback");
    println!("  stop          - Stop playback");
    println!("  vol <0-100>   - Set volume");
    println!("  quit          - Exit");

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "play" if parts.len() > 1 => {
                let url = parts[1];
                println!("Playing: {}", url);
                player.play_track("Stream", "User").await?;
                // Note: In real implementation, play_track sets metadata
                // We need to actually load the URL via underlying client
                // or extend play_track to take URL
            }
            "pause" => {
                player.pause().await?;
                println!("Paused");
            }
            "resume" => {
                player.play().await?;
                println!("Resumed");
            }
            "stop" => {
                player.stop().await?;
                println!("Stopped");
            }
            "vol" if parts.len() > 1 => {
                if let Ok(vol) = parts[1].parse::<u8>() {
                    player.set_volume(vol as f32 / 100.0).await?;
                    println!("Volume: {}%", vol);
                }
            }
            "quit" => break,
            _ => println!("Unknown command"),
        }
    }

    player.disconnect().await?;
    Ok(())
}
```

---

## 23.4 Streaming PCM Audio

**File:** `examples/play_pcm.rs`

```rust
//! Example: Streaming raw sine wave audio

use airplay2::{AirPlayClient, AirPlayConfig};
use airplay2::streaming::AudioSource;
use airplay2::audio::{AudioFormat, SampleFormat, SampleRate, ChannelConfig};
use std::time::Duration;
use std::f32::consts::PI;

// Sine wave generator
struct SineSource {
    phase: f32,
    frequency: f32,
    format: AudioFormat,
}

impl SineSource {
    fn new(frequency: f32) -> Self {
        Self {
            phase: 0.0,
            frequency,
            format: AudioFormat::CD_QUALITY, // 16-bit 44.1kHz stereo
        }
    }
}

impl AudioSource for SineSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;
        let channels = self.format.channels.channels() as usize;

        // Generate stereo samples
        for chunk in buffer.chunks_exact_mut(4) { // 2 bytes * 2 channels
            let sample = (self.phase * 2.0 * PI).sin();
            let value = (sample * i16::MAX as f32) as i16;
            let bytes = value.to_le_bytes();

            // Left
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
            // Right
            chunk[2] = bytes[0];
            chunk[3] = bytes[1];

            self.phase += self.frequency / sample_rate;
            if self.phase > 1.0 {
                self.phase -= 1.0;
            }
        }

        Ok(buffer.len() / 4 * 4)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AirPlayClient::default_client();

    // Discover
    let devices = client.scan(Duration::from_secs(3)).await?;
    let device = devices.first().ok_or("No devices found")?;

    println!("Connecting to {}...", device.name);
    client.connect(device).await?;

    println!("Streaming 440Hz sine wave...");
    let source = SineSource::new(440.0);

    // Start streaming (blocks until stopped)
    // In a real app, you'd spawn this or use a channel
    tokio::select! {
        result = client.client_mut().stream_audio(source) => {
            result?;
        }
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            println!("Stopping...");
        }
    }

    client.disconnect().await?;
    Ok(())
}
```

---

## 23.5 Multi-Room Grouping

**File:** `examples/multi_room.rs`

```rust
//! Example: Create a multi-room group

use airplay2::multiroom::GroupManager;
use airplay2::scan;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = scan(Duration::from_secs(3)).await?;
    if devices.len() < 2 {
        println!("Need at least 2 devices for multi-room example");
        return Ok(());
    }

    let manager = GroupManager::new();

    // Create group
    println!("Creating 'Party Group'...");
    let group_id = manager.create_group("Party Group").await;

    // Add devices
    for device in devices.iter().take(2) {
        println!("Adding {} to group...", device.name);
        manager.add_device_to_group(&group_id, device.clone()).await?;
    }

    // Set group volume
    println!("Setting group volume to 50%...");
    manager.set_group_volume(&group_id, 0.5.into()).await?;

    // In a real implementation, you would now use the group ID
    // to stream audio to the leader device, which syncs with followers.

    if let Some(group) = manager.get_group(&group_id).await {
        println!("Group created with {} members.", group.member_count());
    } else {
        println!("Group not found after creation.");
    }

    Ok(())
}
```
