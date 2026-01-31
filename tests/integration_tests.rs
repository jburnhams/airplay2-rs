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

        // Capture stdout for monitoring
        let stdout = process.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = process.stderr.take().ok_or("Failed to capture stderr")?;

        // Wait for receiver to start by reading output
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);
        let mut output_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        #[allow(unused_assignments)]
        let mut found_serving = false;

        loop {
            if start.elapsed() > timeout {
                let _ = process.kill();
                let stderr_output: String = stderr_lines.join("\n");
                return Err(format!(
                    "Python receiver failed to start within timeout.\nStderr: {}",
                    stderr_output
                )
                .into());
            }

            // Check if process is still running
            match process.try_wait() {
                Ok(Some(status)) => {
                    let stderr_output: String = stderr_lines.join("\n");
                    return Err(format!(
                        "Python receiver exited early: {}\nStderr: {}",
                        status, stderr_output
                    )
                    .into());
                }
                Ok(None) => {
                    // Still running, good
                }
                Err(e) => {
                    return Err(format!("Failed to check receiver status: {}", e).into());
                }
            }

            // Try to read a line from stdout (non-blocking via try_wait check)
            let mut line = String::new();
            if let Ok(n) = reader.read_line(&mut line) {
                if n > 0 {
                    tracing::debug!("Receiver stdout: {}", line.trim());
                    output_lines.push(line.clone());

                    // Check for "serving on" message
                    if line.contains("serving on") {
                        tracing::info!("✓ Python receiver started: {}", line.trim());
                        found_serving = true;
                        break;
                    }
                }
            }

            // Also check stderr for errors
            let mut err_line = String::new();
            if let Ok(n) = stderr_reader.read_line(&mut err_line) {
                if n > 0 {
                    tracing::warn!("Receiver stderr: {}", err_line.trim());
                    stderr_lines.push(err_line);
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        if !found_serving {
            let _ = process.kill();
            return Err("Failed to find 'serving on' message from receiver".into());
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

    /// Verify sine wave quality with frequency analysis
    fn verify_sine_wave_quality(&self, expected_frequency: f32) -> Result<(), Box<dyn std::error::Error>> {
        let audio = self
            .audio_data
            .as_ref()
            .ok_or("No audio data for verification")?;

        // Basic sanity checks
        if audio.len() < 10000 {
            return Err(format!("Audio too short: {} bytes", audio.len()).into());
        }

        // Parse as 16-bit stereo samples (44100 Hz, 2 channels, 16-bit)
        let mut samples = Vec::new();
        let mut min_sample = i16::MAX;
        let mut max_sample = i16::MIN;
        let mut zero_crossings = 0;
        let mut prev_sample = 0i16;

        for chunk in audio.chunks_exact(4) {
            if chunk.len() == 4 {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(left as f32);
                min_sample = min_sample.min(left);
                max_sample = max_sample.max(left);

                // Count zero crossings
                if (prev_sample < 0 && left >= 0) || (prev_sample >= 0 && left < 0) {
                    zero_crossings += 1;
                }
                prev_sample = left;
            }
        }

        let num_samples = samples.len();
        let sample_rate = 44100.0;
        let duration = num_samples as f32 / sample_rate;

        tracing::info!(
            "Audio stats - samples: {}, duration: {:.2}s, min: {}, max: {}, range: {}",
            num_samples,
            duration,
            min_sample,
            max_sample,
            max_sample - min_sample
        );

        // 1. Verify amplitude range
        let amplitude_range = max_sample - min_sample;
        if amplitude_range < 20000 {
            return Err(format!(
                "Audio amplitude too low: {} (expected >20000 for full-scale sine wave)",
                amplitude_range
            ).into());
        }

        // 2. Verify dynamic range (should use most of 16-bit range)
        if max_sample.abs() < 25000 || min_sample.abs() < 25000 {
            tracing::warn!(
                "Audio not using full dynamic range: max={}, min={}",
                max_sample,
                min_sample
            );
        }

        // 3. Estimate frequency from zero crossings
        // Zero crossings per second = frequency * 2 (one crossing per half cycle)
        let estimated_frequency = (zero_crossings as f32 / duration) / 2.0;
        let frequency_error = (estimated_frequency - expected_frequency).abs();
        let frequency_tolerance = expected_frequency * 0.05; // 5% tolerance

        tracing::info!(
            "Frequency analysis - expected: {}Hz, estimated: {:.1}Hz, error: {:.1}Hz, tolerance: {:.1}Hz",
            expected_frequency,
            estimated_frequency,
            frequency_error,
            frequency_tolerance
        );

        if frequency_error > frequency_tolerance {
            return Err(format!(
                "Frequency mismatch: expected {}Hz, got {:.1}Hz (error: {:.1}Hz > {:.1}Hz tolerance)",
                expected_frequency,
                estimated_frequency,
                frequency_error,
                frequency_tolerance
            ).into());
        }

        // 4. Check for continuity (shouldn't have long runs of zeros)
        let mut max_zero_run = 0;
        let mut current_zero_run = 0;

        for &sample in &samples {
            if sample.abs() < 100.0 {  // Near-zero threshold
                current_zero_run += 1;
                max_zero_run = max_zero_run.max(current_zero_run);
            } else {
                current_zero_run = 0;
            }
        }

        if max_zero_run > (sample_rate as usize / 10) {
            return Err(format!(
                "Found suspicious silence: {} consecutive near-zero samples",
                max_zero_run
            ).into());
        }

        // 5. Simple FFT-based frequency verification (optional, more accurate)
        // For now, zero-crossing is sufficient and doesn't require additional deps

        tracing::info!("✓ Audio quality verified: {}Hz sine wave with good amplitude and continuity", expected_frequency);
        Ok(())
    }

    /// Detailed audio analysis (for debugging)
    #[allow(dead_code)]
    fn analyze_audio_detailed(&self) -> Result<AudioAnalysis, Box<dyn std::error::Error>> {
        let audio = self
            .audio_data
            .as_ref()
            .ok_or("No audio data for analysis")?;

        let mut samples = Vec::new();
        for chunk in audio.chunks_exact(4) {
            if chunk.len() == 4 {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(left as f32);
            }
        }

        let num_samples = samples.len();
        let sample_rate = 44100.0;

        // Calculate RMS (loudness)
        let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / num_samples as f32).sqrt();

        // Calculate peak amplitude
        let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);

        // Calculate crest factor (peak/rms ratio)
        let crest_factor = if rms > 0.0 { peak / rms } else { 0.0 };

        // Count zero crossings for frequency estimation
        let mut zero_crossings = 0;
        for i in 1..samples.len() {
            if (samples[i-1] < 0.0 && samples[i] >= 0.0) || (samples[i-1] >= 0.0 && samples[i] < 0.0) {
                zero_crossings += 1;
            }
        }

        let duration = num_samples as f32 / sample_rate;
        let estimated_frequency = (zero_crossings as f32 / duration) / 2.0;

        Ok(AudioAnalysis {
            num_samples,
            duration,
            sample_rate,
            rms,
            peak,
            crest_factor,
            estimated_frequency,
            zero_crossings,
        })
    }
}

#[allow(dead_code)]
struct AudioAnalysis {
    num_samples: usize,
    duration: f32,
    sample_rate: f32,
    rms: f32,
    peak: f32,
    crest_factor: f32,
    estimated_frequency: f32,
    zero_crossings: usize,
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
