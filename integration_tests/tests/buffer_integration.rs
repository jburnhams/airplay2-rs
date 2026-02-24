use std::time::Duration;

use airplay2::audio::{AudioFormat, ChannelConfig, SampleFormat, SampleRate};
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig, AirPlayDevice};

use crate::common::python_receiver::PythonReceiver;

mod common;

async fn setup_streaming_test(
    buffer_frames: usize,
) -> Result<(PythonReceiver, AirPlayClient, AirPlayDevice), Box<dyn std::error::Error>> {
    let receiver = PythonReceiver::start().await?;

    // Give receiver time to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    let config = AirPlayConfig {
        discovery_timeout: Duration::from_secs(5),
        connection_timeout: Duration::from_secs(5),
        audio_buffer_frames: buffer_frames,
        ..Default::default()
    };

    let client = AirPlayClient::new(config);
    let device = receiver.device_config();
    client.connect(&device).await?;

    Ok((receiver, client, device))
}

struct FiniteSineWaveSource {
    phase: f64,
    freq: f64,
    sample_rate: u32,
    channels: u8,
    samples_generated: usize,
    max_samples: usize,
}

impl FiniteSineWaveSource {
    fn new(freq: f64, sample_rate: u32, channels: u8, duration: Duration) -> Self {
        let max_samples =
            (duration.as_secs_f64() * f64::from(sample_rate) * f64::from(channels)) as usize;
        Self {
            phase: 0.0,
            freq,
            sample_rate,
            channels,
            samples_generated: 0,
            max_samples,
        }
    }
}

impl AudioSource for FiniteSineWaveSource {
    fn format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: SampleRate::from_hz(self.sample_rate).unwrap(),
            channels: if self.channels == 1 {
                ChannelConfig::Mono
            } else {
                ChannelConfig::Stereo
            },
            sample_format: SampleFormat::I16,
        }
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.samples_generated >= self.max_samples {
            return Ok(0);
        }

        let mut samples_written = 0;
        let bytes_per_sample = 2;
        let frame_size = bytes_per_sample * self.channels as usize;

        for chunk in buffer.chunks_mut(frame_size) {
            if chunk.len() < frame_size {
                break;
            }

            if self.samples_generated >= self.max_samples {
                break;
            }

            let value = (self.phase * 2.0 * std::f64::consts::PI).sin();
            let sample = (value * 30000.0) as i16;

            for ch in 0..self.channels {
                let start = ch as usize * 2;
                chunk[start..start + 2].copy_from_slice(&sample.to_le_bytes());
                self.samples_generated += 1;
            }

            self.phase += self.freq / f64::from(self.sample_rate);
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }

            samples_written += 1;
        }

        Ok(samples_written * frame_size)
    }

    fn is_seekable(&self) -> bool {
        false
    }

    fn seek(&mut self, _pos: Duration) -> std::io::Result<()> {
        Err(std::io::Error::other("Not seekable"))
    }
}

#[tokio::test]
async fn test_streaming_with_small_buffer() {
    // 100ms buffer (4410 frames at 44.1kHz)
    let buffer_frames = 4410;
    let (_receiver, mut client, _device) = setup_streaming_test(buffer_frames)
        .await
        .expect("Failed to setup test");

    // Stream for 2 seconds
    let source = FiniteSineWaveSource::new(440.0, 44100, 2, Duration::from_secs(2));

    let result = client.stream_audio(source).await;
    assert!(
        result.is_ok(),
        "Streaming failed with small buffer: {:?}",
        result.err()
    );

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_streaming_with_large_buffer() {
    // 2s buffer (88200 frames at 44.1kHz)
    let buffer_frames = 88200;
    let (_receiver, mut client, _device) = setup_streaming_test(buffer_frames)
        .await
        .expect("Failed to setup test");

    // Stream for 3 seconds (to ensure we fill buffer and stream)
    let source = FiniteSineWaveSource::new(880.0, 44100, 2, Duration::from_secs(3));

    let result = client.stream_audio(source).await;
    assert!(
        result.is_ok(),
        "Streaming failed with large buffer: {:?}",
        result.err()
    );

    client.disconnect().await.expect("Failed to disconnect");
}
