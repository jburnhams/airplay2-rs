//! Integration test for NTP timing synchronization fallback

use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::audio::AudioCodec;
use airplay2::{AirPlayClient, AirPlayConfig, TimingProtocol};
use common::python_receiver::{PythonReceiver, ReceiverOutput, TestSineSource};

#[tokio::test]
async fn test_ntp_synchronization() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting NTP Synchronization integration test");

    // 1. Start Receiver (default configuration enables PTP)
    // Use --fakemac to avoid potential issues with all-zero MACs on loopback in CI
    // We disable PTP master and override server version here, so the Python receiver allows
    // fallback to NTP.
    let receiver = PythonReceiver::start_with_args(&[
        "--fakemac",
        "--no-ptp-master",
        "--server-version",
        "350.0",
        "--debug",
    ])
    .await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client with NTP enabled explicitly
    tracing::info!("Connecting with NTP timing protocol...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::Pcm) // Use PCM for simplicity
        .timing_protocol(TimingProtocol::Ntp) // Explicitly request NTP
        .pin("3939")
        .build();

    let mut client = AirPlayClient::new(config);

    let mut connected = false;
    let mut last_error = None;
    for _ in 0..3 {
        if let Err(e) = client.connect(&device).await {
            tracing::error!("Connection failed: {}", e);
            last_error = Some(e);
            sleep(Duration::from_secs(2)).await;
        } else {
            connected = true;
            break;
        }
    }

    if !connected {
        let output = receiver.stop().await?;
        if output.log_path.exists() {
            let logs = std::fs::read_to_string(&output.log_path)?;
            println!("Receiver Logs (Connection Failed):\n{}", logs);
        }
        return Err(last_error.unwrap().into());
    }

    assert!(client.is_connected().await, "Client should be connected");

    // 3. Stream Audio for enough time to exchange NTP TimeAnnounce messages
    tracing::info!("Streaming audio to trigger TimeAnnounce exchange...");
    let source = TestSineSource::new(440.0, 5.0); // 5 seconds of audio

    // Stream (blocks until done or error)
    if let Err(e) = client.stream_audio(source).await {
        tracing::error!("Streaming failed: {}", e);
        let output = receiver.stop().await?;
        if output.log_path.exists() {
            let logs = std::fs::read_to_string(&output.log_path)?;
            println!("Receiver Logs (Streaming Failed):\n{}", logs);
        }
        return Err(e.into());
    }

    // 4. Stop and Verify
    client.disconnect().await?;
    let output: ReceiverOutput = receiver.stop().await?;

    // Verify audio received (sanity check)
    output.verify_audio_received()?;

    // 5. Analyze Logs for NTP activity
    if output.log_path.exists() {
        let logs = std::fs::read_to_string(&output.log_path)?;

        let has_time_announce = logs.contains("TIME_ANNOUNCE_NTP");

        if has_time_announce {
            tracing::info!("✓ Receiver logs contain 'TIME_ANNOUNCE_NTP'");
        } else {
            tracing::error!("Receiver logs DO NOT contain 'TIME_ANNOUNCE_NTP'");
            println!("Receiver Logs:\n{}", logs);
        }
        assert!(
            has_time_announce,
            "Receiver logs should contain 'TIME_ANNOUNCE_NTP'"
        );
    } else {
        tracing::warn!("Receiver log file not found!");
    }

    tracing::info!("✓ NTP Synchronization test completed");
    Ok(())
}
