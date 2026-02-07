//! Integration tests for Volume and Pause controls
//!
//! Verifies that client commands are correctly received and processed by the Python receiver.

use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, audio::AudioFormat};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;

/// Wrapper for the Python AirPlay 2 receiver
struct PythonReceiver {
    process: Child,
    logs: Arc<Mutex<Vec<String>>>,
}

impl PythonReceiver {
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let output_dir = std::env::current_dir()?.join("airplay2-receiver");
        let interface = std::env::var("AIRPLAY_TEST_INTERFACE").unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "lo0".to_string()
            } else {
                "lo".to_string()
            }
        });

        println!("Starting Python receiver on interface: {}", interface);

        let mut process = Command::new("python3")
            .arg("ap2-receiver.py")
            .arg("--netiface")
            .arg(&interface)
            .current_dir(&output_dir)
            .env("AIRPLAY_FILE_SINK", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let logs_clone = logs.clone();

        // Spawn output capture tasks
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        let logs_stdout = logs.clone();
        let logs_stderr = logs.clone();

        tokio::spawn(async move {
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                // println!("[Receiver Out]: {}", line);
                logs_stdout.lock().unwrap().push(line);
            }
        });

        tokio::spawn(async move {
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                // println!("[Receiver Err]: {}", line);
                logs_stderr.lock().unwrap().push(line);
            }
        });

        // Wait for startup
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(10) {
                return Err("Timeout waiting for receiver to start".into());
            }

            {
                let captured_logs = logs_clone.lock().unwrap();
                if captured_logs.iter().any(|l| l.contains("serving on")) {
                    break;
                }
            }
            sleep(Duration::from_millis(100)).await;
        }

        Ok(Self { process, logs })
    }

    async fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        loop {
            if start.elapsed() > timeout {
                return Err(format!("Timeout waiting for log pattern: '{}'", pattern));
            }

            {
                let logs = self.logs.lock().unwrap();
                if logs.iter().any(|l| l.contains(pattern)) {
                    return Ok(());
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn stop(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.process.kill().await?;
        Ok(())
    }

    fn device_config(&self) -> airplay2::AirPlayDevice {
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
    let client = AirPlayClient::default_client();
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
    // Verify log: "rate': 0.0" inside a dictionary log or similar
    // The log is: {'rate': 0.0, 'rtpTime': ...}
    receiver
        .wait_for_log("'rate': 0.0", Duration::from_secs(5))
        .await?;

    // 6. Resume
    println!("Resuming...");
    client.play().await?;
    // Verify log: "rate': 1.0"
    receiver
        .wait_for_log("'rate': 1.0", Duration::from_secs(5))
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
