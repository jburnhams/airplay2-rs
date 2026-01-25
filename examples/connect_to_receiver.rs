//! Example: Connect to the Python test receiver
//!
//! This example discovers devices and connects to our airplay2-receiver test instance.
//! Run the receiver first: `./test_receiver.sh`

use airplay2::audio::AudioFormat;
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

        // Check if we've generated enough samples
        if self.samples_generated >= self.max_samples {
            return Ok(0); // EOF
        }

        let mut bytes_written = 0;
        // Generate stereo samples (4 bytes per sample: 2 bytes left, 2 bytes right)
        for chunk in buffer.chunks_exact_mut(4) {
            if self.samples_generated >= self.max_samples {
                break;
            }

            let sample = (self.phase * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value = (sample * i16::MAX as f32 * 0.5) as i16; // 50% volume
            let bytes = value.to_be_bytes();

            // Left channel
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
            // Right channel
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
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("airplay2=debug".parse().unwrap()),
        )
        .init();

    println!("=== AirPlay 2 Test Client ===");
    println!();

    // 1. Scan for devices
    println!("Scanning for AirPlay devices (5 seconds)...");
    let devices = scan(Duration::from_secs(5)).await?;

    if devices.is_empty() {
        println!("No AirPlay devices found!");
        println!("Make sure the test receiver is running: ./test_receiver.sh");
        return Ok(());
    }

    println!("\nFound {} device(s):", devices.len());
    for (i, device) in devices.iter().enumerate() {
        println!(
            "  [{}] {} at {}:{} (AirPlay2: {})",
            i + 1,
            device.name,
            device.address(),
            device.port,
            device.supports_airplay2()
        );
    }

    // 2. Find our test receiver (try multiple name patterns)
    let target_device = devices
        .iter()
        .find(|d| {
            let name_lower = d.name.to_lowercase();
            name_lower.contains("airplay2-rs-test")
                || name_lower.contains("airplay2-receiver")
                || name_lower.contains("test")
        })
        .or_else(|| devices.first());

    let device = match target_device {
        Some(d) => d,
        None => {
            println!("No suitable device found");
            return Ok(());
        }
    };

    println!(
        "\nConnecting to: {} ({}:{})",
        device.name,
        device.address(),
        device.port
    );

    // 3. Create client and connect
    let config = AirPlayConfig::default();
    let mut client = AirPlayClient::new(config);

    match client.connect(device).await {
        Ok(()) => println!("Connected successfully!"),
        Err(e) => {
            println!("Connection failed: {:?}", e);
            return Err(e.into());
        }
    }

    // 4. Stream a test tone
    println!("\nStreaming 440Hz sine wave for 5 seconds...");
    let source = SineWaveSource::new(440.0, 5);

    match tokio::time::timeout(Duration::from_secs(10), client.stream_audio(source)).await {
        Ok(Ok(())) => println!("Streaming completed successfully!"),
        Ok(Err(e)) => println!("Streaming error: {:?}", e),
        Err(_) => println!("Streaming timed out"),
    }

    // 5. Disconnect
    println!("\nDisconnecting...");
    client.disconnect().await?;
    println!("Done!");

    Ok(())
}
