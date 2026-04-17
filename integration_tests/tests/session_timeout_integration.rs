//! Integration test for session timeout and refresh logic
//!
//! Verifies that the client properly tracks connection state,
//! uses keep-alive to maintain the session, and correctly handles
//! connection drops and session timeouts.

use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::state::ClientEvent;
use airplay2::{AirPlayClient, AirPlayConfig};
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_session_timeout() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Session Timeout test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client
    tracing::info!("Connecting client...");
    let config = AirPlayConfig::builder()
        .pin("3939")
        // Use a very short timeout to trigger timeout handling quickly
        .connection_timeout(Duration::from_secs(2))
        .build();

    let client = AirPlayClient::new(config);

    // Use retry logic for initial connection to handle potential startup flakiness
    let mut connected = false;
    for _ in 0..3 {
        if client.connect(&device).await.is_ok() {
            connected = true;
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !connected {
        return Err("Failed to connect client after retries".into());
    }

    assert!(client.is_connected().await, "Client should be connected");

    let mut rx = client.subscribe_events();

    // The client sends keep-alive (GET /info) requests every 1 second
    // Let's ensure the connection stays alive for a few keep-alive cycles
    sleep(Duration::from_secs(3)).await;

    // Check that we're still connected, meaning keep-alive succeeded
    assert!(
        client.is_connected().await,
        "Client should still be connected via keep-alive"
    );

    // 3. Force disconnect receiver without clean shutdown
    tracing::info!("Killing receiver to simulate network loss or device crash...");
    receiver.stop().await?;

    // 4. Wait for Disconnected event
    // The client should detect the connection is lost due to keep-alive failure
    // or TCP connection drop within a few seconds
    tracing::info!("Waiting for Disconnected event...");
    let event = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Disconnected { reason, .. }) => {
                    tracing::info!("Received Disconnected event: {}", reason);
                    // The reason should indicate a network error or keep-alive failure
                    return Ok(());
                }
                Ok(e) => tracing::debug!("Ignored event: {:?}", e),
                Err(e) => return Err(format!("Recv error: {}", e)),
            }
        }
    })
    .await;

    match event {
        Ok(Ok(())) => {
            tracing::info!("✓ Session timeout detected successfully");
            assert!(
                !client.is_connected().await,
                "Client should report disconnected"
            );
            Ok(())
        }
        Ok(Err(e)) => Err(format!("Event receiver error: {}", e).into()),
        Err(_) => Err("Timeout waiting for Disconnected event".into()),
    }
}
