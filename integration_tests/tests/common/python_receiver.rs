use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;

/// Python receiver wrapper for testing
pub struct PythonReceiver {
    process: Child,
    output_dir: PathBuf,
    #[allow(dead_code)]
    interface: String,
}

impl PythonReceiver {
    /// Start the Python receiver
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_args(&[]).await
    }

    /// Start the Python receiver with additional arguments
    pub async fn start_with_args(args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
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

        // Clean up pairings for fresh state (added for persistent pairing test)
        let pairings_dir = output_dir.join("pairings");
        if pairings_dir.exists() {
            fs::remove_dir_all(&pairings_dir)?;
        }
        fs::create_dir_all(&pairings_dir)?;

        // Restore .gitignore to keep repo clean
        fs::write(pairings_dir.join(".gitignore"), "*\n!.gitignore\n")?;

        tracing::info!("Starting Python receiver on interface: {}", interface);
        tracing::debug!("Current dir: {:?}", std::env::current_dir());
        tracing::debug!("Output dir: {:?}", output_dir);
        tracing::debug!("Script path: {:?}", output_dir.join("ap2-receiver.py"));

        // Use "python" instead of "python3" to ensure we use the active environment
        // (e.g. from venv or setup-python in CI).
        // On Windows, "python" is standard. On Linux/macOS, "python" usually links to the active version.
        let mut command = Command::new("python");
        command
            .arg("ap2-receiver.py")
            .arg("--netiface")
            .arg(&interface);

        for arg in args {
            command.arg(arg);
        }

        let mut process = command
            .current_dir(&output_dir)
            .env("AIRPLAY_FILE_SINK", "1")
            .env("AIRPLAY_SAVE_RTP", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn python process: {}", e))?;

        // Capture stdout for monitoring
        let stdout = process.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = process.stderr.take().ok_or("Failed to capture stderr")?;

        // Wait for receiver to start by reading output
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        let mut reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        let mut stderr_lines = Vec::new();
        #[allow(unused_assignments)]
        let mut found_serving = false;

        loop {
            if start.elapsed() > timeout {
                // process.kill() is async in tokio, but we are returning error which drops process.
                // Since we set kill_on_drop(true), it should be fine.
                // But explicitly killing is good.
                let _ = process.kill().await;
                let stderr_output: String = stderr_lines.join("\n");
                return Err(format!(
                    "Python receiver failed to start within timeout.\nStderr: {}",
                    stderr_output
                )
                .into());
            }

            // Check if process is still running
            if let Ok(Some(status)) = process.try_wait() {
                let stderr_output: String = stderr_lines.join("\n");
                return Err(format!(
                    "Python receiver exited early: {}\nStderr: {}",
                    status, stderr_output
                )
                .into());
            }

            tokio::select! {
                line = reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            tracing::debug!("Receiver stdout: {}", line.trim());
                            if line.contains("serving on") {
                                tracing::info!("✓ Python receiver started: {}", line.trim());
                                found_serving = true;
                                break;
                            }
                        }
                        Ok(None) => {
                            // EOF
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stdout: {}", e);
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                     match line {
                        Ok(Some(line)) => {
                            tracing::warn!("Receiver stderr: {}", line.trim());
                            if line.contains("serving on") {
                                tracing::info!("✓ Python receiver started (detected in stderr): {}", line.trim());
                                found_serving = true;
                                break;
                            }
                            stderr_lines.push(line);
                        }
                        Ok(None) => {
                            // EOF
                        }
                        Err(e) => {
                            tracing::warn!("Error reading stderr: {}", e);
                        }
                    }
                }
                _ = sleep(Duration::from_millis(100)) => {
                    // Continue loop to check timeout/status
                }
            }
        }

        if !found_serving {
            let _ = process.kill().await;
            return Err("Failed to find 'serving on' message from receiver".into());
        }

        // Spawn a background task to keep reading output to prevent deadlocks/SIGPIPE
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(line)) => tracing::debug!("Receiver stdout: {}", line.trim()),
                            Ok(None) => break, // EOF
                            Err(_) => break,
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => tracing::warn!("Receiver stderr: {}", line.trim()),
                            Ok(None) => break, // EOF
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Ok(Self {
            process,
            output_dir,
            interface,
        })
    }

    /// Stop the receiver and read output
    pub async fn stop(mut self) -> Result<ReceiverOutput, Box<dyn std::error::Error>> {
        tracing::info!("Stopping Python receiver");

        // Send SIGTERM to allow graceful shutdown
        #[cfg(unix)]
        {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            if let Some(id) = self.process.id() {
                let pid = Pid::from_raw(id as i32);
                let _ = kill(pid, Signal::SIGTERM);
            }
        }

        #[cfg(windows)]
        {
            let _ = self.process.kill().await;
        }

        // Wait for process to exit
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            let _ = self.process.wait().await;
        })
        .await;

        // Force kill if still running
        let _ = self.process.kill().await;
        let _ = self.process.wait().await;

        // Read output files
        let audio_path = self.output_dir.join("received_audio_44100_2ch.raw");
        let rtp_path = self.output_dir.join("rtp_packets.bin");

        let audio_data = fs::read(&audio_path).ok();
        let rtp_data = fs::read(&rtp_path).ok();

        // Save logs for debugging
        let log_path = PathBuf::from("target").join(format!(
            "integration-test-{}.log",
            chrono::Utc::now().timestamp()
        ));
        if let Some(ref data) = audio_data {
            tracing::info!("Read {} bytes from {}", data.len(), audio_path.display());
        }

        Ok(ReceiverOutput {
            audio_data,
            rtp_data,
            log_path,
        })
    }

    /// Get a device configuration for connecting
    pub fn device_config(&self) -> airplay2::AirPlayDevice {
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
pub struct ReceiverOutput {
    pub audio_data: Option<Vec<u8>>,
    pub rtp_data: Option<Vec<u8>>,
    #[allow(dead_code)]
    pub log_path: PathBuf,
}

impl ReceiverOutput {
    /// Verify audio data meets minimum requirements
    pub fn verify_audio_received(&self) -> Result<(), Box<dyn std::error::Error>> {
        let audio = self.audio_data.as_ref().ok_or("No audio data received")?;

        if audio.is_empty() {
            return Err("Audio data is empty".into());
        }

        tracing::info!("Verified audio data: {} bytes", audio.len());
        Ok(())
    }

    /// Verify RTP packets were received
    pub fn verify_rtp_received(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rtp = self.rtp_data.as_ref().ok_or("No RTP data received")?;

        if rtp.is_empty() {
            return Err("RTP data is empty".into());
        }

        tracing::info!("Verified RTP data: {} bytes", rtp.len());
        Ok(())
    }

    /// Verify sine wave quality with frequency analysis
    pub fn verify_sine_wave_quality(
        &self,
        expected_frequency: f32,
        check_frequency: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
            (max_sample as i32) - (min_sample as i32)
        );

        // 1. Verify amplitude range
        let amplitude_range = (max_sample as i32) - (min_sample as i32);
        if amplitude_range < 20000 {
            return Err(format!(
                "Audio amplitude too low: {} (expected >20000 for full-scale sine wave)",
                amplitude_range
            )
            .into());
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

        if check_frequency && frequency_error > frequency_tolerance {
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
            if sample.abs() < 100.0 {
                // Near-zero threshold
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
            )
            .into());
        }

        // 5. Simple FFT-based frequency verification (optional, more accurate)
        // For now, zero-crossing is sufficient and doesn't require additional deps

        tracing::info!(
            "✓ Audio quality verified: {}Hz sine wave with good amplitude and continuity",
            expected_frequency
        );
        Ok(())
    }

    /// Detailed audio analysis (for debugging)
    #[allow(dead_code)]
    pub fn analyze_audio_detailed(&self) -> Result<AudioAnalysis, Box<dyn std::error::Error>> {
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
            if (samples[i - 1] < 0.0 && samples[i] >= 0.0)
                || (samples[i - 1] >= 0.0 && samples[i] < 0.0)
            {
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
pub struct AudioAnalysis {
    pub num_samples: usize,
    pub duration: f32,
    pub sample_rate: f32,
    pub rms: f32,
    pub peak: f32,
    pub crest_factor: f32,
    pub estimated_frequency: f32,
    pub zero_crossings: usize,
}

/// Sine wave audio source for testing
pub struct TestSineSource {
    phase: f32,
    frequency: f32,
    format: airplay2::audio::AudioFormat,
    samples_generated: usize,
    max_samples: usize,
}

impl TestSineSource {
    pub fn new(frequency: f32, duration_secs: f32) -> Self {
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

    pub fn new_with_sample_rate(frequency: f32, duration_secs: f32, sample_rate: u32) -> Self {
        let format = airplay2::audio::AudioFormat {
            sample_rate: match sample_rate {
                48000 => airplay2::audio::SampleRate::Hz48000,
                44100 => airplay2::audio::SampleRate::Hz44100,
                _ => airplay2::audio::SampleRate::Hz44100, // Default fallback
            },
            channels: airplay2::audio::ChannelConfig::Stereo,
            sample_format: airplay2::audio::SampleFormat::I16,
        };
        let max_samples = (sample_rate as f32 * duration_secs) as usize;

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
