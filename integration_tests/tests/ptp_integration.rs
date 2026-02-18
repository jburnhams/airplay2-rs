//! Integration test for PTP timing synchronization

use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::audio::AudioCodec;
use airplay2::{AirPlayClient, AirPlayConfig, TimingProtocol};
use common::python_receiver::{PythonReceiver, ReceiverOutput, TestSineSource};

#[tokio::test]
async fn test_ptp_synchronization() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting PTP Synchronization integration test");

    // 1. Start Receiver (default configuration enables PTP)
    let receiver = PythonReceiver::start().await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client with PTP enabled
    tracing::info!("Connecting with PTP timing protocol...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::Pcm) // Use PCM for simplicity
        .timing_protocol(TimingProtocol::Ptp) // Explicitly request PTP
        .pin("3939")
        .build();

    let mut client = AirPlayClient::new(config);
    if let Err(e) = client.connect(&device).await {
        tracing::error!("Connection failed: {}", e);
        let output = receiver.stop().await?;
        if output.log_path.exists() {
            let logs = std::fs::read_to_string(&output.log_path)?;
            println!("Receiver Logs (Connection Failed):\n{}", logs);
        }
        return Err(e.into());
    }
    assert!(client.is_connected().await, "Client should be connected");

    // 3. Stream Audio for enough time to exchange PTP messages
    // PTP Sync interval is 1s, Announce is 2s. We need at least 3-5 seconds.
    tracing::info!("Streaming audio to trigger PTP exchange...");
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

    // 5. Analyze Logs for PTP activity
    if output.log_path.exists() {
        let logs = std::fs::read_to_string(&output.log_path)?;

        // Check for PTP related keywords in Receiver logs
        // Based on grep: "PTPInfo", "time_announce_ptp" (case insensitive usually in logs?)
        // The receiver logs might be: "Detected PTP clock..." or similar.
        // Also "SETPEERS" request from client.

        let has_ptp = logs.to_lowercase().contains("ptp");
        let has_setpeers = logs.contains("SETPEERS");
        let has_time_announce = logs.contains("TIME_ANNOUNCE_PTP");

        if has_ptp {
            tracing::info!("✓ Receiver logs contain 'PTP'");
        } else {
            tracing::error!("Receiver logs DO NOT contain 'PTP'. Logs:\n{}", logs);
        }
        assert!(has_ptp, "Receiver logs should contain 'PTP'");

        if has_setpeers {
            tracing::info!("✓ Receiver logs contain 'SETPEERS'");
        } else {
            tracing::error!("Receiver logs DO NOT contain 'SETPEERS'");
        }
        assert!(has_setpeers, "Receiver logs should contain 'SETPEERS'");

        if has_time_announce {
            tracing::info!("✓ Receiver logs contain 'TIME_ANNOUNCE_PTP'");
        } else {
            tracing::error!("Receiver logs DO NOT contain 'TIME_ANNOUNCE_PTP'");
        }
        assert!(
            has_time_announce,
            "Receiver logs should contain 'TIME_ANNOUNCE_PTP'"
        );

        // Assert that we at least tried to use PTP
        // Note: The python receiver might not log "PTP" explicitly if debug logging isn't high
        // enough, but "SETPEERS" is a method name so it should appear if we sent it.
        // And if we are PTP master, we should see "PTP master" logs in OUR output (which we can't
        // easily capture here inside the test, but we can see in console).

        // Let's check for specific receiver log messages found in grep:
        // "Using PTP, here is what is necessary"
        // "should resolve to the device mac and PTP clock port"

        // Also check if we received any audio.
    } else {
        tracing::warn!("Receiver log file not found!");
    }

    tracing::info!("✓ PTP Synchronization test completed");
    Ok(())
}
