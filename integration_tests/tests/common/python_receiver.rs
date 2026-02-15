use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;
use walkdir::WalkDir;

/// Python receiver wrapper for testing
pub struct PythonReceiver {
    process: Child,
    output_dir: PathBuf,
    #[allow(dead_code)]
    interface: String,
    log_buffer: Arc<Mutex<Vec<String>>>,
    // Keep temp dir alive until receiver is dropped
    _temp_dir: Option<TempDir>,
    // Detected port
    port: u16,
    // Flag to ensure logs are written once
    logs_written: bool,
}

impl PythonReceiver {
    /// Start the Python receiver
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_args(&[]).await
    }

    /// Start the Python receiver with additional arguments
    pub async fn start_with_args(args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        // Locate source directory
        let mut source_dir = std::env::current_dir()?.join("airplay2-receiver");
        #[allow(clippy::collapsible_if)]
        if !source_dir.exists() {
            if let Some(parent) = std::env::current_dir()?.parent() {
                let parent_dir = parent.join("airplay2-receiver");
                if parent_dir.exists() {
                    source_dir = parent_dir;
                }
            }
        }

        if !source_dir.exists() {
            return Err("Could not find airplay2-receiver source directory".into());
        }

        // Create temporary directory for this test instance
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().to_path_buf();

        // Copy source files to temp dir
        Self::copy_dir_all(&source_dir, &output_dir)?;

        let interface = std::env::var("AIRPLAY_TEST_INTERFACE").unwrap_or_else(|_| {
            // Use loopback interface for CI
            if cfg!(target_os = "macos") {
                "lo0".to_string()
            } else {
                "lo".to_string()
            }
        });

        // Clean up pairings for fresh state (in temp dir)
        let pairings_dir = output_dir.join("pairings");
        if pairings_dir.exists() {
            fs::remove_dir_all(&pairings_dir)?;
        }
        fs::create_dir_all(&pairings_dir)?;

        // Restore .gitignore to keep repo clean (though irrelevant in temp, good practice)
        fs::write(pairings_dir.join(".gitignore"), "*\n!.gitignore\n")?;

        tracing::info!("Starting Python receiver on interface: {}", interface);
        tracing::debug!("Source dir: {:?}", source_dir);
        tracing::debug!("Output/Temp dir: {:?}", output_dir);
        tracing::debug!("Script path: {:?}", output_dir.join("ap2-receiver.py"));

        let python_exe = std::env::var("PYTHON_EXECUTABLE").unwrap_or_else(|_| {
            if cfg!(windows) {
                "python".to_string()
            } else {
                "python3".to_string()
            }
        });

        let mut command = Command::new(&python_exe);
        command
            .arg("ap2-receiver.py")
            .arg("--netiface")
            .arg(&interface);

        // Check if port is already in args
        if !args.contains(&"-p") && !args.contains(&"--port") {
            command.arg("-p").arg("0");
        }

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
            .map_err(|e| format!("Failed to spawn python3 process: {}", e))?;

        // Capture stdout for monitoring
        let stdout = process.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = process.stderr.take().ok_or("Failed to capture stderr")?;

        let log_buffer = Arc::new(Mutex::new(Vec::new()));
        let log_buffer_clone = log_buffer.clone();

        // Wait for receiver to start by reading output
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);

        let mut reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        #[allow(unused_assignments)]
        let mut found_serving = false;
        let mut actual_port = 7000; // Default fallback

        loop {
            if start.elapsed() > timeout {
                let _ = process.kill().await;
                let logs = log_buffer.lock().unwrap().join("\n");
                return Err(format!(
                    "Python receiver failed to start within timeout.\nLogs:\n{}",
                    logs
                )
                .into());
            }

            // Check if process is still running
            if let Ok(Some(status)) = process.try_wait() {
                let logs = log_buffer.lock().unwrap().join("\n");
                return Err(
                    format!("Python receiver exited early: {}\nLogs:\n{}", status, logs).into(),
                );
            }

            tokio::select! {
                line = reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            tracing::debug!("Receiver stdout: {}", line.trim());
                            if let Ok(mut logs) = log_buffer.lock() {
                                logs.push(format!("STDOUT: {}", line));
                            }
                            if line.contains("serving on") {
                                tracing::info!("✓ Python receiver started: {}", line.trim());
                                found_serving = true;
                                #[allow(clippy::collapsible_if)]
                                if let Some(port_str) = line.split(':').next_back() {
                                    if let Ok(p) = port_str.trim().parse::<u16>() {
                                        actual_port = p;
                                        tracing::info!("Detected receiver port: {}", actual_port);
                                    }
                                }
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("Error reading stdout: {}", e),
                    }
                }
                line = stderr_reader.next_line() => {
                     match line {
                        Ok(Some(line)) => {
                            tracing::warn!("Receiver stderr: {}", line.trim());
                            if let Ok(mut logs) = log_buffer.lock() {
                                logs.push(format!("STDERR: {}", line));
                            }
                            if line.contains("serving on") {
                                tracing::info!(
                                    "✓ Python receiver started (detected in stderr): {}",
                                    line.trim()
                                );
                                found_serving = true;
                                #[allow(clippy::collapsible_if)]
                                if let Some(port_str) = line.split(':').next_back() {
                                    if let Ok(p) = port_str.trim().parse::<u16>() {
                                        actual_port = p;
                                        tracing::info!("Detected receiver port: {}", actual_port);
                                    }
                                }
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("Error reading stderr: {}", e),
                    }
                }
                _ = sleep(Duration::from_millis(100)) => {}
            }
        }

        if !found_serving {
            let _ = process.kill().await;
            return Err("Failed to find 'serving on' message from receiver".into());
        }

        // Spawn a background task to keep reading output
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                tracing::debug!("Receiver stdout: {}", line.trim());
                                if let Ok(mut logs) = log_buffer_clone.lock() {
                                    logs.push(format!("STDOUT: {}", line));
                                }
                            }
                            Ok(None) => break, // EOF
                            Err(_) => break,
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                tracing::warn!("Receiver stderr: {}", line.trim());
                                if let Ok(mut logs) = log_buffer_clone.lock() {
                                    logs.push(format!("STDERR: {}", line));
                                }
                            }
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
            log_buffer,
            _temp_dir: Some(temp_dir),
            port: actual_port,
            logs_written: false,
        })
    }

    fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
        for entry in WalkDir::new(src) {
            let entry = entry?;
            let path = entry.path();
            let path_str = path.to_string_lossy();

            // Skip __pycache__, .git, and pairings
            if path_str.contains("__pycache__")
                || path_str.contains(".git")
                || path_str.contains("pairings")
            {
                continue;
            }

            let relative_path = path.strip_prefix(src).map_err(std::io::Error::other)?;
            let target_path = dst.join(relative_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(&target_path)?;
            } else {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(path, &target_path)?;
            }
        }
        Ok(())
    }

    /// Wait for a log pattern to appear in stdout/stderr
    pub async fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(format!("Timeout waiting for log pattern: '{}'", pattern));
            }

            #[allow(clippy::collapsible_if)]
            if let Ok(guard) = self.log_buffer.lock() {
                if guard.iter().any(|line| line.contains(pattern)) {
                    return Ok(());
                }
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    fn write_logs(&mut self) {
        if self.logs_written {
            return;
        }

        // Write to root/target/integration-test-TIMESTAMP.log
        // If we are in integration_tests crate, root is ../
        // But the current dir is usually where we ran cargo test from.
        // If run from workspace root, current_dir is root.
        // If run from integration_tests, current_dir is integration_tests.

        let mut target_dir = match std::env::current_dir() {
            Ok(pb) => pb,
            Err(_) => PathBuf::from("."),
        };

        // If we are in integration_tests, go up one level?
        // But workspace target dir is usually shared.
        // If running `cargo test -p integration_tests`, it might put artifacts in `target`.
        // Let's try to find the `target` directory.
        if !target_dir.join("target").exists()
            && target_dir
                .parent()
                .map(|p| p.join("target").exists())
                .unwrap_or(false)
        {
            target_dir = target_dir.parent().unwrap().to_path_buf();
        }

        let log_dir = target_dir.join("target");
        // Ensure log dir exists
        if !log_dir.exists() {
            let _ = fs::create_dir_all(&log_dir);
        }

        let log_path = log_dir.join(format!(
            "integration-test-{}.log",
            chrono::Utc::now().timestamp_millis()
        ));

        if let Ok(logs) = self.log_buffer.lock() {
            if let Err(e) = fs::write(&log_path, logs.join("\n")) {
                tracing::warn!(
                    "Failed to write integration test logs to {:?}: {}",
                    log_path,
                    e
                );
            } else {
                tracing::info!("Wrote integration test logs to: {:?}", log_path);
            }
        }
        self.logs_written = true;
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
        let ptp_path = self.output_dir.join("ntp.bin");

        let audio_data = fs::read(&audio_path).ok();
        let rtp_data = fs::read(&rtp_path).ok();
        let ptp_data = fs::read(&ptp_path).ok();

        self.write_logs();

        // Return log path relative to where we think it is?
        // We constructed it in write_logs but didn't store it.
        // Reconstruct for return.
        let mut target_dir = match std::env::current_dir() {
            Ok(pb) => pb,
            Err(_) => PathBuf::from("."),
        };
        if !target_dir.join("target").exists()
            && target_dir
                .parent()
                .map(|p| p.join("target").exists())
                .unwrap_or(false)
        {
            target_dir = target_dir.parent().unwrap().to_path_buf();
        }
        let log_path = target_dir.join("target").join(format!(
            "integration-test-{}.log",
            // Note: timestamp will be slightly different if we call now() again.
            // Ideally we should store the path in self.
            // But for now, we just want logs written.
            // The return value is used for manual inspection.
            "UNKNOWN"
        ));

        if let Some(ref data) = audio_data {
            tracing::info!("Read {} bytes from {}", data.len(), audio_path.display());
        }
        if let Some(ref data) = ptp_data {
            tracing::info!("Read {} bytes from {}", data.len(), ptp_path.display());
        }

        Ok(ReceiverOutput {
            audio_data,
            rtp_data,
            ptp_data,
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
            port: self.port, // Use detected port
            capabilities: airplay2::DeviceCapabilities {
                airplay2: true,
                supports_transient_pairing: true,
                supports_buffered_audio: true, // Python receiver expects stream type 96 or 103
                supports_ptp: true,            // AirPlay 2 implies PTP
                ..Default::default()
            },
            raop_port: None,
            raop_capabilities: None,
            txt_records: HashMap::new(),
        }
    }
}

impl Drop for PythonReceiver {
    fn drop(&mut self) {
        if !self.logs_written {
            self.write_logs();
        }
    }
}

/// Output from the Python receiver
pub struct ReceiverOutput {
    pub audio_data: Option<Vec<u8>>,
    pub rtp_data: Option<Vec<u8>>,
    pub ptp_data: Option<Vec<u8>>,
    #[allow(dead_code)]
    pub log_path: PathBuf,
}

impl ReceiverOutput {
    /// Verify PTP data was received
    pub fn verify_ptp_received(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ptp = self.ptp_data.as_ref().ok_or("No PTP data received")?;

        if ptp.is_empty() {
            return Err("PTP data is empty".into());
        }

        tracing::info!("Verified PTP data: {} bytes", ptp.len());
        Ok(())
    }

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
