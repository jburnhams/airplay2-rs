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

use std::sync::Once;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::python_receiver::{PythonReceiver, TestSineSource};

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

#[tokio::test]
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
    output.verify_sine_wave_quality(440.0, false)?;

    tracing::info!("✅ PCM integration test passed");
    Ok(())
}

#[tokio::test]
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
    output.verify_sine_wave_quality(440.0, true)?;

    tracing::info!("✅ ALAC integration test passed");
    Ok(())
}

#[tokio::test]
async fn test_custom_pin_pairing() -> Result<(), Box<dyn std::error::Error>> {
    init();
    tracing::info!("Starting Custom PIN Pairing integration test");

    // Start Python receiver
    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(3)).await;

    let device = receiver.device_config();

    // Test 0: Sanity check with default client (implicit PIN logic)
    tracing::info!("Test 0: Connecting with default client (sanity check)...");
    let client_default = airplay2::AirPlayClient::default_client();
    client_default.connect(&device).await?;
    client_default.disconnect().await?;
    tracing::info!("✅ Default client connected successfully");

    sleep(Duration::from_millis(500)).await;

    // Test 1: Connect with CORRECT PIN (3939)
    tracing::info!("Test 1: Connecting with correct PIN (3939)...");
    let config = airplay2::AirPlayConfig::builder().pin("3939").build();

    let client = airplay2::AirPlayClient::new(config);
    match client.connect(&device).await {
        Ok(_) => {
            assert!(client.is_connected().await);
            tracing::info!("✅ Connected successfully with correct PIN");
            client.disconnect().await?;
        }
        Err(e) => {
            tracing::error!("Failed to connect with correct PIN: {:?}", e);
            // Dump logs for debugging
            let output = receiver.stop().await?;
            if let Ok(logs) = std::fs::read_to_string(&output.log_path) {
                println!("Receiver Logs:\n{}", logs);
            }
            return Err(e.into());
        }
    }

    sleep(Duration::from_millis(500)).await;

    // Test 2: Connect with WRONG PIN (0000)
    tracing::info!("Test 2: Connecting with wrong PIN (0000)...");
    let config_wrong = airplay2::AirPlayConfig::builder().pin("0000").build();
    let client_wrong = airplay2::AirPlayClient::new(config_wrong);

    match client_wrong.connect(&device).await {
        Ok(_) => {
            // Cleanup
            let _ = receiver.stop().await;
            return Err("Client connected with wrong PIN! This should fail.".into());
        }
        Err(e) => {
            tracing::info!(
                "✅ Client failed to connect with wrong PIN as expected: {}",
                e
            );
        }
    }

    // Stop receiver
    let _ = receiver.stop().await?;
    Ok(())
}
