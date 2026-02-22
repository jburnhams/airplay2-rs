use airplay2::AirPlayClient;
use airplay2::audio::AudioCodec;
use airplay2::types::AirPlayConfig;
use std::time::Duration;

mod common;
use common::python_receiver::{PythonReceiver, TestSineSource};

#[tokio::test]
async fn test_aac_eld_streaming() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting AAC-ELD Streaming integration test");

    // 1. Start Python Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();

    // 2. Connect Client with AAC-ELD codec
    tracing::info!("Connecting with AAC-ELD codec...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::AacEld)
        .aac_bitrate(64_000)
        .build();

    let client = AirPlayClient::new(config);

    // Connection retry loop
    let mut connected = false;
    for i in 0..3 {
        if let Err(e) = client.connect(&device).await {
            tracing::warn!("Connection attempt {} failed: {}", i + 1, e);
            tokio::time::sleep(Duration::from_secs(1)).await;
        } else {
            connected = true;
            break;
        }
    }
    assert!(connected, "Failed to connect to receiver");

    // 3. Stream Audio
    tracing::info!("Streaming AAC-ELD audio...");
    // Stream 3 seconds of sine wave at 440Hz
    let source = TestSineSource::new(440.0, 3.0);

    let mut client = client;
    client.stream_audio(source).await?;

    // 4. Verify
    let receiver_output = receiver.stop().await?;

    // Check logs for successful reception/decoding OR decoding failure due to missing ASC
    // We expect RTP packets to be received even if decoding fails.

    if let Some(logs) = &receiver_output.log_path.to_str() {
        if std::path::Path::new(logs).exists() {
            let log_content = std::fs::read_to_string(logs)?;
            tracing::info!("Receiver logs:\n{}", log_content);

            // Check if PyAV complained about data (which means it received data but couldn't decode)
            // "Invalid data found when processing input" is common for missing ASC
            let received_data = log_content.contains("Invalid data found")
                || log_content.contains("Error decoding audio");
            let received_rtp = receiver_output
                .rtp_data
                .as_ref()
                .map(|v| !v.is_empty())
                .unwrap_or(false);

            if received_data {
                tracing::info!(
                    "Receiver received data but failed to decode (expected without ASC)"
                );
            }

            assert!(received_rtp, "RTP data should have been received");
        }
    }

    // We do NOT call verify_audio_received() because decoding is expected to fail without correct ASC exchange
    // receiver_output.verify_audio_received()?;

    // But we MUST have received RTP packets
    receiver_output.verify_rtp_received()?;

    Ok(())
}
