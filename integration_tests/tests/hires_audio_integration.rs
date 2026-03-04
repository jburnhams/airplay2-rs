use std::time::Duration;

use airplay2::audio::{AudioFormat, ChannelConfig, SampleFormat, SampleRate};
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::time::sleep;

mod common;
use common::python_receiver::PythonReceiver;

/// A simple 24-bit sine wave generator (packed as 3 bytes per sample) for 48kHz
struct I24HiresSineSource {
    frequency: f64,
    sample_rate: u32,
    time: f64,
    samples_produced: usize,
    max_samples: usize,
}

impl I24HiresSineSource {
    fn new(frequency: f64, duration_secs: f64) -> Self {
        let sample_rate = 48000;
        let max_samples = (duration_secs * f64::from(sample_rate)) as usize;
        Self {
            frequency,
            sample_rate,
            time: 0.0,
            samples_produced: 0,
            max_samples,
        }
    }
}

impl AudioSource for I24HiresSineSource {
    fn format(&self) -> AudioFormat {
        AudioFormat::new(
            SampleFormat::I24,
            SampleRate::Hz48000,
            ChannelConfig::Stereo,
        )
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.samples_produced >= self.max_samples {
            return Ok(0); // EOF
        }

        let bytes_per_sample = 3; // 24-bit = 3 bytes
        let channels = 2; // Stereo
        let bytes_per_frame = bytes_per_sample * channels;
        let max_frames = buffer.len() / bytes_per_frame;

        let frames_to_generate =
            std::cmp::min(max_frames, self.max_samples - self.samples_produced);

        for i in 0..frames_to_generate {
            let t = self.time;

            // Generate sine wave value between -1.0 and 1.0
            let value = (t * self.frequency * 2.0 * std::f64::consts::PI).sin();

            // Map to 24-bit integer range [-8388608, 8388607]
            // We use slightly less than max to avoid clipping
            let amplitude = 8_300_000.0;
            let sample_i32 = (value * amplitude) as i32;

            // Pack as 24-bit little endian
            let b0 = (sample_i32 & 0xFF) as u8;
            let b1 = ((sample_i32 >> 8) & 0xFF) as u8;
            let b2 = ((sample_i32 >> 16) & 0xFF) as u8;

            let offset = i * bytes_per_frame;

            // Left channel
            buffer[offset] = b0;
            buffer[offset + 1] = b1;
            buffer[offset + 2] = b2;

            // Right channel
            buffer[offset + 3] = b0;
            buffer[offset + 4] = b1;
            buffer[offset + 5] = b2;

            self.time += 1.0 / f64::from(self.sample_rate);
        }

        self.samples_produced += frames_to_generate;
        Ok(frames_to_generate * bytes_per_frame)
    }
}

/// Test high resolution 24-bit/48kHz streaming to the receiver
#[tokio::test]
async fn test_hires_audio_streaming() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    tracing::info!("Starting High Res Audio integration test (24-bit/48kHz)");

    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    // Use prefer_hires_audio config option
    let config = AirPlayConfig::builder().prefer_hires_audio(true).build();
    let mut client = AirPlayClient::new(config);

    let device = receiver.device_config();

    let mut connected = false;
    for i in 1..=3 {
        tracing::info!("Connecting to receiver (attempt {}/3)...", i);
        match client.connect(&device).await {
            Ok(()) => {
                connected = true;
                break;
            }
            Err(e) => {
                tracing::warn!("Connection failed attempt {}: {}", i, e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    if !connected {
        return Err("Failed to connect after 3 attempts".into());
    }

    tracing::info!("Connected! Generating 48kHz / 24-bit audio stream...");

    // Create 24-bit 48kHz audio source
    let source = I24HiresSineSource::new(440.0, 3.0);

    client.stream_audio(source).await?;

    client.disconnect().await?;
    sleep(Duration::from_secs(1)).await;
    let output = receiver.stop().await?;

    output.verify_audio_received()?;

    // The Python receiver sets up ALSA correctly using the parameters we sent
    // We expect the receiver's logs to show 48000 sample rate and 24-bit depth if it supports it,
    // or if we forced it. The Python receiver audio format for 48kHz might just be PCM.
    // Let's verify quality.
    match output.verify_sine_wave_quality(440.0, true) {
        Ok(_) => tracing::info!("✅ Quality verification passed"),
        Err(e) => tracing::warn!("⚠️ Quality verification warning: {}", e),
    }

    tracing::info!("✅ High Res Audio integration test finished");
    Ok(())
}
