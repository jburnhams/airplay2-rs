//! Example: Persistent pairing test
//!
//! This example demonstrates persistent pairing by:
//! 1. Connecting to the receiver (performs Pair-Setup and saves keys)
//! 2. Disconnecting
//! 3. Reconnecting (should use Pair-Verify with stored keys)

use airplay2::audio::AudioFormat;
use airplay2::protocol::pairing::storage::FileStorage;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig, scan};
use std::f32::consts::PI;
use std::time::Duration;

/// Simple sine wave generator for testing
struct SineWaveSource {
    phase: f32,
    frequency: f32,
    format: AudioFormat,
    samples_generated: u64,
    max_samples: u64,
}

impl SineWaveSource {
    fn new(frequency: f32, duration_secs: u32) -> Self {
        let format = AudioFormat::CD_QUALITY; // 16-bit 44.1kHz stereo
        let max_samples = u64::from(duration_secs) * u64::from(format.sample_rate.as_u32());
        Self {
            phase: 0.0,
            frequency,
            format,
            samples_generated: 0,
            max_samples,
        }
    }
}

impl AudioSource for SineWaveSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;

        if self.samples_generated >= self.max_samples {
            return Ok(0); // EOF
        }

        let mut bytes_written = 0;
        for chunk in buffer.chunks_exact_mut(4) {
            if self.samples_generated >= self.max_samples {
                break;
            }

            let sample = (self.phase * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value = (sample * i16::MAX as f32 * 0.5) as i16;
            let bytes = value.to_be_bytes(); // Big Endian for AirPlay

            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
            chunk[2] = bytes[0];
            chunk[3] = bytes[1];

            self.phase += self.frequency / sample_rate;
            if self.phase > 1.0 {
                self.phase -= 1.0;
            }

            self.samples_generated += 1;
            bytes_written += 4;
        }

        Ok(bytes_written)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("airplay2=debug")
        .init();

    println!("=== Persistent Pairing Test ===");

    // Initialize storage
    let storage_path = "pairings.json";
    // Remove existing storage to ensure fresh start
    if std::path::Path::new(storage_path).exists() {
        std::fs::remove_file(storage_path)?;
    }

    let storage = Box::new(FileStorage::new(storage_path).await?);

    // Create client with storage
    let config = AirPlayConfig::default();
    let mut client = AirPlayClient::new(config).with_pairing_storage(storage);

    println!("Scanning for devices...");
    let devices = scan(Duration::from_secs(3)).await?;

    let target_device = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains("receiver"))
        .or_else(|| devices.first());

    let device = match target_device {
        Some(d) => d.clone(),
        None => {
            println!("No device found");
            return Ok(());
        }
    };

    println!("Found device: {}", device.name);

    // First connection (Pair-Setup)
    println!("\n--- Attempt 1: Pair-Setup ---");
    client.connect(&device).await?;
    println!("Connected! Streaming briefly...");

    let source1 = SineWaveSource::new(440.0, 2);
    client.stream_audio(source1).await?;

    println!("Disconnecting...");
    client.disconnect().await?;

    // Verify storage file exists
    if std::path::Path::new(storage_path).exists() {
        println!("Success: pairings.json created.");
    } else {
        println!("Error: pairings.json NOT created.");
    }

    // Second connection (Pair-Verify)
    println!("\n--- Attempt 2: Pair-Verify ---");
    // We reuse the same client instance which holds the storage
    // In a real app, loading the storage from disk on restart would achieve the same

    client.connect(&device).await?;
    println!("Connected (should be fast)! Streaming briefly...");

    let source2 = SineWaveSource::new(880.0, 2);
    client.stream_audio(source2).await?;

    println!("Disconnecting...");
    client.disconnect().await?;

    println!("Test complete!");
    Ok(())
}
