//! Example: Verify Volume and Pause
//!
//! This example connects to a receiver, sets volume, streams audio, pauses, resumes, changes volume, and then stops.

use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, scan};
use std::f32::consts::PI;
use std::time::Duration;
use tokio::time::sleep;

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

        // Generate stereo samples
        for chunk in buffer.chunks_exact_mut(4) {
            // 2 bytes * 2 channels
            let sample = (self.phase * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
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
    tracing_subscriber::fmt::init();

    // Discover
    println!("Scanning for devices...");
    let devices = scan(Duration::from_secs(3)).await?;
    let device = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains("receiver"))
        .or_else(|| devices.first())
        .ok_or("No devices found")?
        .clone();

    println!("Connecting to {}...", device.name);
    let client = AirPlayClient::default_client();
    client.connect(&device).await?;

    // 1. Set Initial Volume
    println!("Setting volume to 50%...");
    client.set_volume(0.5).await?;
    sleep(Duration::from_millis(500)).await;

    // 2. Start Streaming
    println!("Starting stream...");
    // We need to run streaming in background because it blocks
    let mut client_clone = client.clone();
    let stream_handle = tokio::spawn(async move {
        let source = SineSource::new(440.0);
        if let Err(e) = client_clone.stream_audio(source).await {
            eprintln!("Streaming error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    // 3. Pause
    println!("Pausing playback...");
    client.pause().await?;
    sleep(Duration::from_secs(2)).await;

    // 4. Resume
    println!("Resuming playback...");
    client.play().await?;
    sleep(Duration::from_secs(2)).await;

    // 5. Change Volume
    println!("Setting volume to 25%...");
    client.set_volume(0.25).await?;
    sleep(Duration::from_secs(1)).await;

    // 6. Stop
    println!("Stopping...");
    client.stop().await?;

    // Abort streamer task if it hasn't finished
    stream_handle.abort();

    client.disconnect().await?;
    println!("Verification sequence complete.");
    Ok(())
}
