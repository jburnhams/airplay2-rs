//! Example: Streaming ALAC-encoded audio

use airplay2::audio::{AudioCodec, AudioFormat};
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig, scan};
use std::f32::consts::PI;
use std::time::Duration;

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
    println!("Found devices:");
    for d in &devices {
        println!(" - {}", d.name);
    }
    let device = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains("receiver"))
        .or_else(|| devices.first())
        .cloned()
        .unwrap_or_else(|| {
            println!("No devices found, trying manual connection to 192.168.0.101...");
            let capabilities = airplay2::DeviceCapabilities {
                airplay2: true,
                supports_transient_pairing: true,
                ..Default::default()
            };

            airplay2::AirPlayDevice {
                id: "Manual".to_string(),
                name: "Manual".to_string(),
                model: None,
                addresses: vec!["192.168.0.101".parse().unwrap()],
                port: 7000,
                capabilities,
                raop_port: None,
                raop_capabilities: None,
                txt_records: std::collections::HashMap::new(),
            }
        });

    println!("Connecting to {} with ALAC codec...", device.name);

    // Configure client to use ALAC
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::Alac)
        .build();

    let mut client = AirPlayClient::new(config);
    client.connect(&device).await?;

    println!("Streaming 440Hz sine wave (ALAC encoded)...");
    let source = SineSource::new(440.0);

    // Start streaming (blocks until stopped)
    tokio::select! {
        result = client.stream_audio(source) => {
            result?;
        }
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            println!("Stopping...");
        }
    }

    client.disconnect().await?;
    println!("Streaming completed successfully!");
    Ok(())
}
