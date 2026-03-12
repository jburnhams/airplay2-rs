use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::audio::AudioCodec;
use airplay2::{AirPlayClient, AirPlayConfig, TimingProtocol};
use common::python_receiver::{PythonReceiver, ReceiverOutput, TestSineSource};

#[tokio::test]
async fn test_multi_room_coordination() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Multi-Room Coordination integration test");

    // Start Receiver. We use `--fakemac` to avoid loopback issues and `--no-ptp-master`
    // to force it to act as a follower if needed, though default Python receiver handles it.
    let receiver = PythonReceiver::start_with_args(&["--fakemac"]).await?;

    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // Connect Client with PTP enabled (required for multi-room)
    tracing::info!("Connecting to receiver...");
    let config = AirPlayConfig::builder()
        .audio_codec(AudioCodec::Pcm)
        .timing_protocol(TimingProtocol::Ptp)
        .pin("3939")
        .build();

    let mut client = AirPlayClient::new(config);
    if let Err(e) = client.connect(&device).await {
        tracing::error!("Connection failed: {}", e);
        receiver.stop().await?;
        return Err(e.into());
    }
    assert!(client.is_connected().await, "Client should be connected");

    // The multi-room coordinator logic in `AirPlayClient` is tied to the PTP synchronization
    // and sending SETRATEANCHORTIME. When `stream_audio` is called with PTP active,
    // the client waits for PTP sync, then sends SETRATEANCHORTIME and RECORD.
    // This integration test verifies that this process completes successfully against a real
    // receiver.

    tracing::info!("Streaming audio to trigger multi-room coordination setup...");
    let source = TestSineSource::new(440.0, 3.0); // 3 seconds of audio

    if let Err(e) = client.stream_audio(source).await {
        tracing::error!("Streaming failed: {}", e);
        receiver.stop().await?;
        return Err(e.into());
    }

    client.disconnect().await?;
    let output: ReceiverOutput = receiver.stop().await?;

    // Verify audio received
    output.verify_audio_received()?;

    // Check logs for multi-room related keywords
    if output.log_path.exists() {
        let logs = std::fs::read_to_string(&output.log_path)?;

        let has_setrateanchortime = logs.contains("SETRATEANCHORTIME");
        let has_record = logs.contains("RECORD");

        assert!(
            has_setrateanchortime,
            "Receiver logs should contain 'SETRATEANCHORTIME' for multi-room/buffered audio"
        );
        assert!(has_record, "Receiver logs should contain 'RECORD'");

        tracing::info!(
            "✓ Receiver logs contain SETRATEANCHORTIME and RECORD, confirming multi-room sync \
             sequence"
        );
    } else {
        tracing::warn!("Receiver log file not found!");
    }

    tracing::info!("✓ Multi-Room Coordination test completed");
    Ok(())
}
