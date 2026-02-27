//! Integration tests for Metadata and Progress updates
//!
//! Verifies that client `set_metadata` (DAAP/DMAP) and `set_progress` are correctly received and processed by the Python receiver.

use std::time::Duration;

use airplay2::protocol::daap::{DmapProgress, TrackMetadata};
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::time::sleep;

mod common;
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_metadata_updates() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Metadata integration test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();

    // 2. Connect
    // Use a longer timeout for CI environments
    let config = AirPlayConfig::builder()
        .connection_timeout(Duration::from_secs(30))
        .build();
    let client = AirPlayClient::new(config);

    tracing::info!("Connecting to receiver...");
    let mut connected = false;
    for i in 0..5 {
        if client.connect(&device).await.is_ok() {
            connected = true;
            break;
        }
        tracing::info!("Connection attempt {} failed, retrying...", i + 1);
        sleep(Duration::from_secs(1)).await;
    }

    if !connected {
        return Err("Failed to connect client after retries".into());
    }
    tracing::info!("Connected!");

    // 3. Send Metadata
    tracing::info!("Sending metadata...");
    let metadata = TrackMetadata::builder()
        .title("Rust AirPlay Integration Test")
        .artist("Ferris the Crab")
        .album("Systems Programming")
        .build();

    client.set_metadata(metadata).await?;

    // Verify logs
    // dxxp.py logs parsed tags: "code: value"
    // minm -> dmap.itemname
    // asar -> daap.songartist
    // asal -> daap.songalbum
    tracing::info!("Verifying metadata logs...");
    receiver
        .wait_for_log(
            "dmap.itemname: Rust AirPlay Integration Test",
            Duration::from_secs(5),
        )
        .await?;
    receiver
        .wait_for_log("daap.songartist: Ferris the Crab", Duration::from_secs(5))
        .await?;
    receiver
        .wait_for_log(
            "daap.songalbum: Systems Programming",
            Duration::from_secs(5),
        )
        .await?;
    tracing::info!("Metadata verified!");

    // 4. Send Progress
    tracing::info!("Sending progress...");
    // Start=0, Current=1000, End=5000 (samples)
    let progress = DmapProgress::new(0, 1000, 5000);
    client.set_progress(progress).await?;

    // Verify log: SET_PARAMETER: b'progress' => b' 0/1000/5000'
    // Note: The python code logs `pp[1]` which includes the leading space from "progress: 0/1000/5000"
    tracing::info!("Verifying progress logs...");
    receiver
        .wait_for_log(
            "SET_PARAMETER: b'progress' => b' 0/1000/5000",
            Duration::from_secs(5),
        )
        .await?;
    tracing::info!("Progress verified!");

    // 5. Cleanup
    tracing::info!("Disconnecting...");
    client.disconnect().await?;
    receiver.stop().await?;

    tracing::info!("✅ Metadata integration test passed");
    Ok(())
}
