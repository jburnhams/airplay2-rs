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

/// Helper to connect with retry logic
async fn connect_with_retry(
    client: &mut airplay2::AirPlayClient,
    device: &airplay2::AirPlayDevice,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut connected = false;
    for _ in 0..3 {
        if client.connect(device).await.is_ok() {
            connected = true;
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }
    if !connected {
        return Err("Failed to connect to receiver after 3 attempts".into());
    }
    Ok(())
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
    connect_with_retry(&mut client, &device).await?;

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
    connect_with_retry(&mut client, &device).await?;

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
async fn test_aac_streaming_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    init();

    tracing::info!("Starting AAC integration test");

    // Start Python receiver
    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    // Create client with AAC codec
    let device = receiver.device_config();
    let config = airplay2::AirPlayConfig::builder()
        .audio_codec(airplay2::audio::AudioCodec::Aac)
        .build();

    let mut client = airplay2::AirPlayClient::new(config);

    tracing::info!("Connecting to receiver with AAC...");
    connect_with_retry(&mut client, &device).await?;

    // Stream 3 seconds of 440Hz sine wave
    tracing::info!("Streaming AAC audio...");
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
    // output.verify_sine_wave_quality(440.0, true)?;

    tracing::info!("✅ AAC integration test passed");
    Ok(())
}

#[tokio::test]
async fn test_custom_pin_pairing() -> Result<(), Box<dyn std::error::Error>> {
    init();
    tracing::info!("Starting Custom PIN Pairing integration test");

    // Start Python receiver
    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    // Test 1: Connect with CORRECT PIN (3939)
    tracing::info!("Test 1: Connecting with correct PIN (3939)...");
    let device = receiver.device_config();
    let config = airplay2::AirPlayConfig::builder().pin("3939").build();

    let mut client = airplay2::AirPlayClient::new(config);
    connect_with_retry(&mut client, &device).await?;
    assert!(client.is_connected().await);
    tracing::info!("✅ Connected successfully with correct PIN");
    client.disconnect().await?;

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
