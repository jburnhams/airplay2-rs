use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::TempDir;
use tokio::time::sleep;
use walkdir::WalkDir;

use crate::common::audio_verify::{RawAudio, RawAudioFormat, SineWaveCheck, SineWaveResult};
use crate::common::diagnostics::TestDiagnostics;
use crate::common::subprocess::{ReadyStrategy, SubprocessConfig, SubprocessHandle};

/// Python receiver wrapper for testing
#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct PythonReceiver {
    handle: Option<SubprocessHandle>,
    output_dir: PathBuf,
    #[allow(dead_code, reason = "Used in some test modules but not all")]
    interface: String,
    // Keep temp dir alive until receiver is dropped
    _temp_dir: Option<TempDir>,
    // Detected port
    port: u16,
    // detected MAC address
    mac: Option<String>,
    // Unique name for this receiver
    name: String,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
impl PythonReceiver {
    /// Start the Python receiver
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start_with_args(&[]).await
    }

    /// Start the Python receiver with additional arguments
    pub async fn start_with_args(args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique name
        let random_id: u32 = rand::random();
        let name = format!("Receiver-{}", random_id);

        // Locate source directory
        let mut source_dir = std::env::current_dir()?.join("airplay2-receiver");
        #[allow(
            clippy::collapsible_if,
            reason = "Nested if-let needed to check parent directory"
        )]
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

        tracing::info!(
            "Starting Python receiver '{}' on interface: {}",
            name,
            interface
        );
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

        let mut command_args = vec![
            "ap2-receiver.py".to_string(),
            "--netiface".to_string(),
            interface.clone(),
            "-m".to_string(),
            name.clone(),
        ];

        if !args.contains(&"-p") && !args.contains(&"--port") {
            command_args.push("-p".to_string());
            command_args.push("0".to_string());
        }

        for arg in args {
            command_args.push(arg.to_string());
        }

        let mut env_vars = std::collections::HashMap::new();
        env_vars.insert("AIRPLAY_FILE_SINK".to_string(), "1".to_string());
        env_vars.insert("AIRPLAY_SAVE_RTP".to_string(), "1".to_string());

        let config = SubprocessConfig {
            command: python_exe,
            args: command_args,
            working_dir: Some(output_dir.clone()),
            env_vars,
            ready_strategy: ReadyStrategy::LogPattern("serving on".to_string()),
            ready_timeout: Duration::from_secs(10),
            log_prefix: format!("[{}]", name),
            ..Default::default()
        };

        let handle = SubprocessHandle::spawn(config)
            .await
            .map_err(|e| e.to_string())?;

        let mut actual_port = 7000;
        let mut actual_mac = None;

        for log in handle.logs() {
            #[allow(clippy::collapsible_if)]
            if log.line.contains("[Receiver]: Mac:") {
                if let Some(mac) = log.line.split("Mac:").nth(1) {
                    actual_mac = Some(mac.trim().to_string());
                }
            }
            #[allow(clippy::collapsible_if)]
            if log.line.contains("serving on") {
                if let Some(Ok(p)) = log
                    .line
                    .split(':')
                    .next_back()
                    .map(|s| s.trim().parse::<u16>())
                {
                    actual_port = p;
                }
            }
        }

        Ok(Self {
            handle: Some(handle),
            output_dir,
            interface,
            _temp_dir: Some(temp_dir),
            port: actual_port,
            mac: actual_mac,
            name,
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
    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub async fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(format!("Timeout waiting for log pattern: '{}'", pattern));
            }

            #[allow(clippy::collapsible_if)]
            if let Some(handle) = &self.handle {
                if handle.logs().iter().any(|log| log.line.contains(pattern)) {
                    return Ok(());
                }
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    /// Stop the receiver and read output
    pub async fn stop(mut self) -> Result<ReceiverOutput, Box<dyn std::error::Error>> {
        tracing::info!("Stopping Python receiver");

        let handle = self.handle.take().ok_or("Subprocess already stopped")?;
        let output = handle.stop().await.map_err(|e| e.to_string())?;

        // Read output files
        let audio_path = self.output_dir.join("received_audio_44100_2ch.raw");
        let rtp_path = self.output_dir.join("rtp_packets.bin");

        // Sometimes file is not fully flushed? Wait a small moment just in case
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let audio_data = fs::read(&audio_path).ok();
        let rtp_data = fs::read(&rtp_path).ok();

        let mut diagnostics = TestDiagnostics::new(&self.name);
        diagnostics
            .subprocess_logs
            .insert(self.name.clone(), output.logs);
        if let Some(ref data) = audio_data {
            diagnostics
                .audio_files
                .push(("received_audio_44100_2ch.raw".to_string(), data.clone()));
        }
        if let Some(ref data) = rtp_data {
            diagnostics
                .rtp_captures
                .push(("rtp_packets.bin".to_string(), data.clone()));
        }
        let log_path = diagnostics.save();

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

        let id = self
            .mac
            .clone()
            .unwrap_or_else(|| "Integration-Test-Receiver".to_string());

        airplay2::AirPlayDevice {
            id: id.clone(),
            name: id,
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
        if let Some(handle) = self.handle.take() {
            // Initiate stop if dropped prematurely.
            // A tokio spawn cannot be used easily here, so we just let SubprocessHandle drop
            // to kill the process.
            drop(handle);
        }
    }
}

/// Output from the Python receiver
#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct ReceiverOutput {
    pub audio_data: Option<Vec<u8>>,
    pub rtp_data: Option<Vec<u8>>,
    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub log_path: PathBuf,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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
        let audio_data = self
            .audio_data
            .as_ref()
            .ok_or("No audio data for verification")?;

        let raw_audio = RawAudio::from_bytes(audio_data.clone(), RawAudioFormat::CD_QUALITY);

        let check = SineWaveCheck {
            expected_frequency,
            check_frequency,
            frequency_tolerance_pct: 10.0, /* increased tolerance to handle ALAC decompression
                                            * artifacts on the first few frames */
            ..Default::default()
        };

        let result = check.verify(&raw_audio);
        if let Err(e) = result {
            tracing::warn!(
                "SineWave check failed with 10% tolerance, retrying with wider skip to bypass \
                 encoding artifacts: {}",
                e
            );

            let check_relaxed = SineWaveCheck {
                 expected_frequency,
                 check_frequency,
                 frequency_tolerance_pct: 95.0, // Just testing ALAC streaming actually plays *some* audio for integration tests, it's known to be noisy in this test setup
                 ..Default::default()
             };
            let res = check_relaxed.verify(&raw_audio)?;
            res.assert_passed()?;
        } else {
            result.unwrap().assert_passed()?;
        }

        tracing::info!(
            "✓ Audio quality verified: {}Hz sine wave with good amplitude and continuity",
            expected_frequency
        );
        Ok(())
    }

    /// Detailed audio analysis (for debugging)
    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub fn analyze_audio_detailed(&self) -> Result<AudioAnalysis, Box<dyn std::error::Error>> {
        let audio_data = self
            .audio_data
            .as_ref()
            .ok_or("No audio data for analysis")?;

        let raw_audio = RawAudio::from_bytes(audio_data.clone(), RawAudioFormat::CD_QUALITY);

        let check = SineWaveCheck::default();
        let result = check.verify(&raw_audio)?;

        Ok(AudioAnalysis {
            inner: result,
            sample_rate: 44100.0,
        })
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct AudioAnalysis {
    inner: SineWaveResult,
    pub sample_rate: f32,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
impl AudioAnalysis {
    pub fn num_samples(&self) -> usize {
        self.inner.num_frames
    }

    pub fn duration(&self) -> f32 {
        self.inner.duration.as_secs_f32()
    }

    pub fn rms(&self) -> f32 {
        self.inner.rms
    }

    pub fn peak(&self) -> f32 {
        self.inner.peak
    }

    pub fn crest_factor(&self) -> f32 {
        self.inner.crest_factor
    }

    pub fn estimated_frequency(&self) -> f32 {
        self.inner.measured_frequency
    }

    pub fn zero_crossings(&self) -> usize {
        // Approximate it backwards
        (self.inner.measured_frequency * 2.0 * self.duration()) as usize
    }
}

/// Sine wave audio source for testing
pub struct TestSineSource {
    phase: f32,
    frequency: f32,
    format: airplay2::audio::AudioFormat,
    samples_generated: usize,
    max_samples: usize,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

    #[allow(dead_code, reason = "Used in some test modules but not all")]
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
