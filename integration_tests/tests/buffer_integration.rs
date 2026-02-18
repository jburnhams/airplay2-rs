use crate::common::python_receiver::PythonReceiver;
use airplay2::audio::{AudioFormat, ChannelConfig, SampleFormat, SampleRate};
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig};
use std::time::Duration;

mod common;

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
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Not seekable",
        ))
    }
}

#[tokio::test]
async fn test_streaming_with_small_buffer() {
    let receiver = PythonReceiver::start()
        .await
        .expect("Failed to start receiver");

    // 100ms buffer (4410 frames at 44.1kHz)
    let buffer_frames = 4410;

    let mut config = AirPlayConfig::default();
    config.discovery_timeout = Duration::from_secs(5);
    config.connection_timeout = Duration::from_secs(5);
    config.audio_buffer_frames = buffer_frames;

    let mut client = AirPlayClient::new(config);
    let device = receiver.device_config();
    client.connect(&device).await.expect("Failed to connect");

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
    let receiver = PythonReceiver::start()
        .await
        .expect("Failed to start receiver");

    // 2s buffer (88200 frames at 44.1kHz)
    let buffer_frames = 88200;

    let mut config = AirPlayConfig::default();
    config.discovery_timeout = Duration::from_secs(5);
    config.connection_timeout = Duration::from_secs(5);
    config.audio_buffer_frames = buffer_frames;

    let mut client = AirPlayClient::new(config);
    let device = receiver.device_config();
    client.connect(&device).await.expect("Failed to connect");

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
