//! Integration tests for Volume and Pause controls
//!
//! Verifies that client commands are correctly received and processed by the Python receiver.

mod common;

use airplay2::{AirPlayClient, AirPlayConfig};
use common::python_receiver::{PythonReceiver, TestSineSource};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_volume_and_pause() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();

    // 2. Connect
    println!("Connecting...");
    // Use configured client with PIN 3939 (default for receiver)
    let config = AirPlayConfig::builder()
        .pin("3939")
        .build();
    let client = AirPlayClient::new(config);
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
        let source = TestSineSource::new(440.0, 10.0);
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
    let _ = receiver.stop().await?;

    println!("✅ Volume and Pause integration test passed");
    Ok(())
}
