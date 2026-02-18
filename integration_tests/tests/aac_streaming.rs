//! Integration test for AAC audio streaming

use std::time::Duration;
use tokio::time::sleep;

mod common;
use airplay2::{AirPlayClient, AirPlayConfig, audio::AudioCodec};
use common::python_receiver::{PythonReceiver, TestSineSource};

#[tokio::test]
async fn test_aac_streaming_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting AAC Streaming integration test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start_with_args(&["-nv"]).await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client with AAC codec
    tracing::info!("Connecting with AAC codec...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::Aac)
        .pin("3939")
        .connection_timeout(Duration::from_secs(10))
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

    // 3. Stream Audio
    tracing::info!("Streaming AAC audio...");
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
    output.verify_audio_received()?;
    output.verify_rtp_received()?;

    if output.log_path.exists() {
        let logs = std::fs::read_to_string(&output.log_path)?;
        if logs.contains("Matched AAC") || logs.contains("AAC_LC") {
            tracing::info!("✓ Receiver confirmed AAC format");
        } else {
            tracing::warn!("Receiver did not explicitly confirm AAC in logs (might be normal)");
        }
    }

    tracing::info!("✓ AAC Streaming test passed");
    Ok(())
}
