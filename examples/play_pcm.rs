//! Example: Streaming raw sine wave audio

use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, scan};
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
        // Each sample is 2 bytes (16-bit)
        // We write 2 channels per frame

        let chunk_size = 4; // 2 bytes * 2 channels
        let chunks = buffer.len() / chunk_size;

        for i in 0..chunks {
            let offset = i * chunk_size;
            let sample = (self.phase * 2.0 * PI).sin();
            let value = (sample * i16::MAX as f32) as i16;
            let bytes = value.to_le_bytes();

            // Left channel
            buffer[offset] = bytes[0];
            buffer[offset + 1] = bytes[1];
            // Right channel
            buffer[offset + 2] = bytes[0];
            buffer[offset + 3] = bytes[1];

            self.phase += self.frequency / sample_rate;
            if self.phase > 1.0 {
                self.phase -= 1.0;
            }
        }

        Ok(chunks * chunk_size)
    }
}

use airplay2::types::AirPlayDevice;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Discover
    println!("Scanning for devices...");
    let mut devices = scan(Duration::from_secs(3)).await?;

    // Manually add Python Receiver
    devices.push(AirPlayDevice {
        name: "MyPythonReceiver".to_string(),
        id: "ac:07:75:12:4a:1f".to_string(), // MAC from logs
        address: "192.168.0.101".parse()?,
        port: 50000,
        model: Some("Receiver".to_string()),
        capabilities: Default::default(),
        txt_records: std::collections::HashMap::new(),
    });

    if devices.is_empty() {
        println!("No devices found.");
        return Ok(());
    }

    println!("Found {} devices.", devices.len());

    // Loop through all devices and try to connect
    for device in &devices {
        println!("\n------------------------------------------------");
        println!(
            "Attempting to connect to: {} ({}) @ {}",
            device.name, device.id, device.address
        );

        let mut device_to_connect = device.clone();

        // Helper to force IPv4 if we can guess it
        // (Simplified logic: if link-local IPv6, try to find matching IPv4 from other entries or just rely on luck)
        // Since `scan` returns whatever mDNS found, we might have multiple entries for same device.
        if device.address.is_ipv6() && device.address.to_string().starts_with("fe80") {
            // See if we have an IPv4 version of this device in the list
            if let Some(v4_dev) = devices
                .iter()
                .find(|d| d.id == device.id && d.address.is_ipv4())
            {
                println!("Switching to IPv4 address: {}", v4_dev.address);
                device_to_connect = v4_dev.clone();
            } else if device.name == "One" {
                // Hardcoded fallback for your known Sonos IP
                println!("Forcing known IPv4 for Sonos One...");
                device_to_connect.address = "192.168.0.130".parse().unwrap();
            }
        }

        let mut client = AirPlayClient::default_client();
        match client.connect(&device_to_connect).await {
            Ok(_) => {
                println!("SUCCESS! Connected to {}.", device.name);

                println!("Streaming 440Hz sine wave...");
                let source = SineSource::new(440.0);

                // Start streaming (blocks until stopped)
                tokio::select! {
                    result = client.stream_audio(source) => {
                        if let Err(e) = result {
                            println!("Streaming error: {}", e);
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {
                        println!("Streaming worked for 3s! Stopping...");
                    }
                }

                let _ = client.disconnect().await;
                // If one works, we can stop or continue. Let's continue to test all.
            }
            Err(e) => {
                println!("FAILED to connect to {}: {}", device.name, e);
            }
        }
    }

    Ok(())
}
