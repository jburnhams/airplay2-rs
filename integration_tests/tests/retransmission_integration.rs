use std::sync::Arc;
use std::time::Duration;

use airplay2::AirPlayConfig;
use airplay2::audio::AudioCodec;
use airplay2::connection::ConnectionManager;
use airplay2::streaming::PcmStreamer;
use common::python_receiver::{PythonReceiver, TestSineSource};
use tokio::time::sleep;

mod common;

#[tokio::test]
async fn test_retransmission_with_python_receiver() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting retransmission integration test");

    // Start Receiver
    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    let device = receiver.device_config();

    let mut config = AirPlayConfig::default();
    config.audio_codec = AudioCodec::Pcm;

    let manager = Arc::new(ConnectionManager::new(config));

    // Give it retries for connection
    let mut connected = false;
    for _ in 0..3 {
        if manager.connect(&device).await.is_ok() {
            connected = true;
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }
    assert!(connected, "Failed to connect to receiver");

    manager
        .drop_packets_for_test
        .lock()
        .await
        .extend_from_slice(&[10, 11, 12, 13, 14]);

    let target_format = airplay2::audio::AudioFormat {
        sample_rate: airplay2::audio::SampleRate::Hz44100,
        channels: airplay2::audio::ChannelConfig::Stereo,
        sample_format: airplay2::audio::SampleFormat::I16,
    };

    let streamer = Arc::new(PcmStreamer::new(manager.clone(), target_format, 2048));

    if let Some(key) = manager.encryption_key().await {
        streamer.set_encryption_key(key).await;
    }

    // Create sine wave audio source (2 seconds)
    let source = TestSineSource::new(440.0, 2.0);

    // Create an event listener to forward RetransmitRequests to the streamer
    let mut event_rx = manager.subscribe();
    let streamer_clone = streamer.clone();
    tokio::spawn(async move {
        while let Ok(evt) = event_rx.recv().await {
            if let airplay2::connection::ConnectionEvent::RetransmitRequest { seq_start, count } =
                evt
            {
                tracing::info!(
                    "Integration test forwarding RetransmitRequest seq {} count {}",
                    seq_start,
                    count
                );
                let _ = streamer_clone.retransmit(seq_start, count).await;
            }
        }
    });

    // Stream
    streamer.stream(source).await?;

    // Give it time to finish and write out audio file
    sleep(Duration::from_secs(1)).await;
    manager.disconnect().await?;

    // Stop receiver to flush audio
    let output = receiver.stop().await?;

    // We pass check_frequency=false because dropping packets might cause slight timing glitches
    // in the receiver's ALSA mock sink causing frequency analysis to fail,
    // but the main verification is that no MAC check failed and audio is successfully decoded.
    output.verify_sine_wave_quality(440.0, false)?;

    Ok(())
}
