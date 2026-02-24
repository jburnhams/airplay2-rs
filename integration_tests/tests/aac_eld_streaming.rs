//! Integration test for AAC-ELD audio streaming

use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::audio::AudioCodec;
use airplay2::{AirPlayClient, AirPlayConfig};
use common::python_receiver::{PythonReceiver, TestSineSource};

#[tokio::test]
async fn test_aac_eld_streaming_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting AAC-ELD Streaming integration test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start_with_args(&["-nv"]).await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client with AAC-ELD codec
    tracing::info!("Connecting with AAC-ELD codec...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::AacEld)
        .pin("3939")
        .connection_timeout(Duration::from_secs(10))
        .build();

    let mut client = AirPlayClient::new(config);

    // Add retry logic for connection (handling potential auth flakes)
    let mut last_error = None;
    let mut connected = false;
    for attempt in 1..=3 {
        tracing::info!("Connection attempt {}/3...", attempt);
        match client.connect(&device).await {
            Ok(_) => {
                connected = true;
                break;
            }
            Err(e) => {
                tracing::warn!("Connection attempt {} failed: {}", attempt, e);
                last_error = Some(e);
                sleep(Duration::from_secs(2)).await;
            }
        }
    }

    if !connected {
        let e = last_error.unwrap();
        tracing::error!("All connection attempts failed. Last error: {}", e);
        let output = receiver.stop().await?;
        if output.log_path.exists() {
            let logs = std::fs::read_to_string(&output.log_path)?;
            println!("Receiver Logs (Connection Failed):\n{}", logs);
        }
        return Err(e.into());
    }
    assert!(client.is_connected().await, "Client should be connected");

    // 3. Stream Audio
    tracing::info!("Streaming AAC-ELD audio...");
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
    let output = receiver.stop().await?;

    // Verify audio received
    if let Err(e) = output.verify_audio_received() {
        if output.log_path.exists() {
            let logs = std::fs::read_to_string(&output.log_path)?;
            println!("Receiver Logs (Audio Verification Failed):\n{}", logs);
        }
        return Err(e.into());
    }
    output.verify_rtp_received()?;

    if output.log_path.exists() {
        let logs = std::fs::read_to_string(&output.log_path)?;
        if logs.contains("AAC") || logs.contains("eld") || logs.contains("ELD") {
            tracing::info!("✓ Receiver confirmed AAC/ELD format");
        } else {
            tracing::warn!("Receiver did not explicitly confirm AAC-ELD in logs (might be normal)");
        }
    }

    tracing::info!("✓ AAC-ELD Streaming test passed");
    Ok(())
}
