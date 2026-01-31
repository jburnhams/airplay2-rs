//! Integration tests for AirPlay 2 client
//!
//! These tests verify the complete end-to-end streaming pipeline by:
//! 1. Starting the Python airplay2-receiver as a subprocess
//! 2. Running the Rust client to stream audio
//! 3. Verifying the received audio output
//!
//! Requirements:
//! - Python 3.7+ with dependencies from airplay2-receiver/requirements.txt
//! - Network interface available (defaults to loopback)

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::time::Duration;
use tokio::time::sleep;

static INIT: Once = Once::new();

/// Initialize test environment
fn init() {
    INIT.call_once(|| {
        // Initialize logging for tests
        let _ = tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_test_writer()
            .try_init();
    });
}

/// Python receiver wrapper for testing
struct PythonReceiver {
    process: Child,
    output_dir: PathBuf,
    #[allow(dead_code)] interface: String,
}

impl PythonReceiver {
    /// Start the Python receiver
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let output_dir = std::env::current_dir()?.join("airplay2-receiver");
        let interface = std::env::var("AIRPLAY_TEST_INTERFACE").unwrap_or_else(|_| {
            // Use loopback interface for CI
            if cfg!(target_os = "macos") {
                "lo0".to_string()
            } else {
                "lo".to_string()
            }
        });

        // Clean up any previous test outputs
        let _ = fs::remove_file(output_dir.join("received_audio_44100_2ch.raw"));
        let _ = fs::remove_file(output_dir.join("rtp_packets.bin"));

        tracing::info!("Starting Python receiver on interface: {}", interface);

        let mut process = Command::new("python")
            .arg("ap2-receiver.py")
            .arg("--netiface")
            .arg(&interface)
            .current_dir(&output_dir)
            .env("AIRPLAY_FILE_SINK", "1")
            .env("AIRPLAY_SAVE_RTP", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Wait for receiver to start (look for "serving on" message)
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            if start.elapsed() > timeout {
                let _ = process.kill();
                return Err("Python receiver failed to start within timeout".into());
            }

            // Check if process is still running
            match process.try_wait() {
                Ok(Some(status)) => {
                    return Err(format!("Python receiver exited early: {}", status).into());
                }
                Ok(None) => {
                    // Still running, good
                }
                Err(e) => {
                    return Err(format!("Failed to check receiver status: {}", e).into());
                }
            }

            sleep(Duration::from_millis(500)).await;

            // In a real implementation, we'd read stdout to check for "serving on" message
            // For now, just wait a reasonable time
            if start.elapsed() > Duration::from_secs(3) {
                tracing::info!("Assuming receiver is ready after 3 seconds");
                break;
            }
        }

        Ok(Self {
            process,
            output_dir,
            interface,
        })
    }

    /// Stop the receiver and read output
    async fn stop(mut self) -> Result<ReceiverOutput, Box<dyn std::error::Error>> {
        tracing::info!("Stopping Python receiver");

        // Send SIGTERM to allow graceful shutdown
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let pid = Pid::from_raw(self.process.id() as i32);
            let _ = kill(pid, Signal::SIGTERM);
        }

        #[cfg(windows)]
        {
            let _ = self.process.kill();
        }

        // Wait for process to exit
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(Some(_)) = self.process.try_wait() {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        // Force kill if still running
        let _ = self.process.kill();
        let _ = self.process.wait();

        // Read output files
        let audio_path = self.output_dir.join("received_audio_44100_2ch.raw");
        let rtp_path = self.output_dir.join("rtp_packets.bin");

        let audio_data = fs::read(&audio_path).ok();
        let rtp_data = fs::read(&rtp_path).ok();

        // Save logs for debugging
        let log_path = PathBuf::from("target")
            .join(format!("integration-test-{}.log", chrono::Utc::now().timestamp()));
        if let Some(ref data) = audio_data {
            tracing::info!(
                "Read {} bytes from {}",
                data.len(),
                audio_path.display()
            );
        }

        Ok(ReceiverOutput {
            audio_data,
            rtp_data,
            log_path,
        })
    }

    /// Get a device configuration for connecting
    fn device_config(&self) -> airplay2::AirPlayDevice {
        use std::collections::HashMap;

        airplay2::AirPlayDevice {
            id: "Integration-Test-Receiver".to_string(),
            name: "Integration-Test-Receiver".to_string(),
            model: Some("AirPlay2-Receiver".to_string()),
            addresses: vec!["127.0.0.1".parse().unwrap()],
            port: 7000,
            capabilities: airplay2::DeviceCapabilities {
                airplay2: true,
                supports_transient_pairing: true,
                ..Default::default()
            },
            raop_port: None,
            raop_capabilities: None,
            txt_records: HashMap::new(),
        }
    }
}

/// Output from the Python receiver
struct ReceiverOutput {
    audio_data: Option<Vec<u8>>,
    rtp_data: Option<Vec<u8>>,
    #[allow(dead_code)] log_path: PathBuf,
}

impl ReceiverOutput {
    /// Verify audio data meets minimum requirements
    fn verify_audio_received(&self) -> Result<(), Box<dyn std::error::Error>> {
        let audio = self
            .audio_data
            .as_ref()
            .ok_or("No audio data received")?;

        if audio.is_empty() {
            return Err("Audio data is empty".into());
        }

        tracing::info!("Verified audio data: {} bytes", audio.len());
        Ok(())
    }

    /// Verify RTP packets were received
    fn verify_rtp_received(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rtp = self.rtp_data.as_ref().ok_or("No RTP data received")?;

        if rtp.is_empty() {
            return Err("RTP data is empty".into());
        }

        tracing::info!("Verified RTP data: {} bytes", rtp.len());
        Ok(())
    }

    /// Verify sine wave quality (basic check)
    fn verify_sine_wave_quality(&self, _frequency: f32) -> Result<(), Box<dyn std::error::Error>> {
        let audio = self
            .audio_data
            .as_ref()
            .ok_or("No audio data for verification")?;

        // Basic sanity checks
        if audio.len() < 10000 {
            return Err(format!("Audio too short: {} bytes", audio.len()).into());
        }

        // Parse as 16-bit stereo samples
        let mut min_sample = i16::MAX;
        let mut max_sample = i16::MIN;
        let mut non_zero = 0;

        for chunk in audio.chunks_exact(4) {
            if chunk.len() == 4 {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]);
                min_sample = min_sample.min(left);
                max_sample = max_sample.max(left);
                if left != 0 {
                    non_zero += 1;
                }
            }
        }

        tracing::info!(
            "Audio stats - min: {}, max: {}, non_zero: {}",
            min_sample,
            max_sample,
            non_zero
        );

        // Verify we have actual audio data (not silence or garbage)
        if max_sample - min_sample < 10000 {
            return Err("Audio amplitude too low (might be silence)".into());
        }

        if non_zero < audio.len() / 8 {
            return Err("Too many zero samples".into());
        }

        Ok(())
    }
}

/// Sine wave audio source for testing
struct TestSineSource {
    phase: f32,
    frequency: f32,
    format: airplay2::audio::AudioFormat,
    samples_generated: usize,
    max_samples: usize,
}

impl TestSineSource {
    fn new(frequency: f32, duration_secs: f32) -> Self {
        let format = airplay2::audio::AudioFormat::CD_QUALITY;
        let max_samples = (format.sample_rate.as_u32() as f32 * duration_secs) as usize;

        Self {
            phase: 0.0,
            frequency,
            format,
            samples_generated: 0,
            max_samples,
        }
    }
}

impl airplay2::streaming::AudioSource for TestSineSource {
    fn format(&self) -> airplay2::audio::AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        use std::f32::consts::PI;

        if self.samples_generated >= self.max_samples {
            return Ok(0); // EOF
        }

        let sample_rate = self.format.sample_rate.as_u32() as f32;
        let mut bytes_written = 0;

        for chunk in buffer.chunks_exact_mut(4) {
            if self.samples_generated >= self.max_samples {
                break;
            }

            let sample = (self.phase * 2.0 * PI).sin();
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

            self.samples_generated += 1;
            bytes_written += 4;
        }

        Ok(bytes_written)
    }
}

#[tokio::test]
#[ignore] // Run with --ignored flag
async fn test_pcm_streaming_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    init();

    tracing::info!("Starting PCM integration test");

    // Start Python receiver
    let receiver = PythonReceiver::start().await?;

    // Give receiver extra time to fully initialize
    sleep(Duration::from_secs(2)).await;

    // Create client and connect
    let device = receiver.device_config();
    let mut client = airplay2::AirPlayClient::default_client();

    tracing::info!("Connecting to receiver...");
    client.connect(&device).await?;

    // Stream 3 seconds of 440Hz sine wave
    tracing::info!("Streaming audio...");
    let source = TestSineSource::new(440.0, 3.0);

    client.stream_audio(source).await?;

    tracing::info!("Disconnecting...");
    client.disconnect().await?;

    // Small delay before stopping receiver
    sleep(Duration::from_secs(1)).await;

    // Stop receiver and collect output
    let output = receiver.stop().await?;

    // Verify results
    output.verify_audio_received()?;
    output.verify_rtp_received()?;
    output.verify_sine_wave_quality(440.0)?;

    tracing::info!("✅ PCM integration test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Run with --ignored flag
async fn test_alac_streaming_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    init();

    tracing::info!("Starting ALAC integration test");

    // Start Python receiver
    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    // Create client with ALAC codec
    let device = receiver.device_config();
    let config = airplay2::AirPlayConfig::builder()
        .audio_codec(airplay2::audio::AudioCodec::Alac)
        .build();

    let mut client = airplay2::AirPlayClient::new(config);

    tracing::info!("Connecting to receiver with ALAC...");
    client.connect(&device).await?;

    // Stream 3 seconds of 440Hz sine wave
    tracing::info!("Streaming ALAC audio...");
    let source = TestSineSource::new(440.0, 3.0);

    client.stream_audio(source).await?;

    tracing::info!("Disconnecting...");
    client.disconnect().await?;

    sleep(Duration::from_secs(1)).await;

    // Stop receiver and collect output
    let output = receiver.stop().await?;

    // Verify results
    output.verify_audio_received()?;
    output.verify_rtp_received()?;
    output.verify_sine_wave_quality(440.0)?;

    tracing::info!("✅ ALAC integration test passed");
    Ok(())
}
