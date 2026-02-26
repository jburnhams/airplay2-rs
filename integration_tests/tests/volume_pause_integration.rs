//! Integration tests for Volume and Pause controls
//!
//! Verifies that client commands are correctly received and processed by the Python receiver.

use std::time::Duration;

use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::time::sleep;
mod common;
use common::python_receiver::PythonReceiver;

// Sine wave generator (reused from verify_volume_pause.rs)
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
async fn test_volume_and_pause() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();

    // 2. Connect
    println!("Connecting...");
    // Use a longer timeout for CI environments where the Python receiver might be slow to accept
    // connections
    let config = AirPlayConfig::builder()
        .connection_timeout(Duration::from_secs(30))
        .build();
    let client = AirPlayClient::new(config);
    client.connect(&device).await?;

    // 3. Set Volume (Initial)
    println!("Setting volume to 0.5 (-6.02 dB)...");
    client.set_volume(0.5).await?;
    // Verify log
    receiver
        .wait_for_log(
            "SET_PARAMETER: b'volume' => b' -6.0206",
            Duration::from_secs(5),
        )
        .await?;

    // 4. Start Streaming (Background)
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

    // 5. Pause
    println!("Pausing...");
    client.pause().await?;
    // Verify log: "rate': 0" inside a dictionary log or similar (Python might print 0.0 as 0)
    // The log is: {'rate': 0.0, 'rtpTime': ...}
    // Increasing timeout to 10s for slow CI
    receiver
        .wait_for_log("'rate': 0", Duration::from_secs(10))
        .await?;

    // 6. Resume
    println!("Resuming...");
    client.play().await?;
    // Verify log: "rate': 1"
    receiver
        .wait_for_log("'rate': 1", Duration::from_secs(10))
        .await?;

    // 7. Change Volume
    println!("Setting volume to 0.25 (-12.04 dB)...");
    client.set_volume(0.25).await?;
    receiver
        .wait_for_log(
            "SET_PARAMETER: b'volume' => b' -12.0412",
            Duration::from_secs(5),
        )
        .await?;

    // 8. Stop
    println!("Stopping...");
    client.stop().await?;
    stream_handle.abort();
    client.disconnect().await?;
    receiver.stop().await?;

    println!("✅ Volume and Pause integration test passed");
    Ok(())
}
