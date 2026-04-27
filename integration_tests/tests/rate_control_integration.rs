//! Integration tests for Rate Control (Fast Forward and Rewind)
//!
//! Verifies that client commands are correctly received and processed by the Python receiver.

use std::time::Duration;

use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::time::sleep;

mod common;
use common::python_receiver::PythonReceiver;

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
            format: AudioFormat::CD_QUALITY,
        }
    }
}

impl AudioSource for SineSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;
        let mut written = 0;
        for chunk in buffer.chunks_exact_mut(4) {
            let sample = (self.phase * 2.0 * std::f32::consts::PI).sin();
            let value = (sample * i16::MAX as f32) as i16;
            let bytes = value.to_le_bytes();
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
            chunk[2] = bytes[0];
            chunk[3] = bytes[1];
            self.phase += self.frequency / sample_rate;
            if self.phase > 1.0 {
                self.phase -= 1.0;
            }
            written += 4;
        }
        Ok(written)
    }
}

#[tokio::test]
async fn test_rate_control() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();

    // 2. Connect
    println!("Connecting...");
    let config = AirPlayConfig::builder()
        .connection_timeout(Duration::from_secs(30))
        .build();
    let client = AirPlayClient::new(config);

    // Use retry logic for robustness in CI
    let mut connected = false;
    for i in 0..3 {
        println!("Connection attempt {}/3...", i + 1);
        if client.connect(&device).await.is_ok() {
            connected = true;
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !connected {
        return Err("Failed to connect client after retries".into());
    }

    // 3. Start Streaming (Background)
    println!("Starting stream...");
    let mut client_clone = client.clone();
    let stream_handle = tokio::spawn(async move {
        let source = SineSource::new(440.0);
        if let Err(e) = client_clone.stream_audio(source).await {
            eprintln!("Streaming error: {:?}", e);
        }
    });

    // Wait a bit for stream to establish
    sleep(Duration::from_secs(2)).await;

    // 4. Fast Forward
    println!("Fast forwarding...");
    client.fast_forward().await?;

    receiver
        .wait_for_log("'rate': 2.0,", Duration::from_secs(15))
        .await?;

    // 5. Rewind
    println!("Rewinding...");
    client.rewind().await?;

    receiver
        .wait_for_log("'rate': -2.0,", Duration::from_secs(15))
        .await?;

    // 6. Stop
    println!("Stopping...");
    client.stop().await?;
    stream_handle.abort();
    client.disconnect().await?;
    receiver.stop().await?;

    println!("✅ Rate control integration test passed");
    Ok(())
}
