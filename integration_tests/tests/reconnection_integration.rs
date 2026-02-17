//! Integration test for reconnection logic

use std::time::Duration;
use tokio::time::sleep;

mod common;
use airplay2::{AirPlayClient, AirPlayConfig, AirPlayPlayer, state::ClientEvent};
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_disconnection_detection() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Disconnection Detection test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 2. Connect Client
    tracing::info!("Connecting client...");
    let config = AirPlayConfig::builder()
        .pin("3939")
        .build();

    let client = AirPlayClient::new(config);
    if let Err(e) = client.connect(&device).await {
        tracing::error!("Connection failed: {}", e);
        return Err(e.into());
    }
    assert!(client.is_connected().await, "Client should be connected");

    // 3. Subscribe to events
    let mut rx = client.subscribe_events();
    tracing::info!("Subscribed to client events");

    // 4. Kill Receiver
    tracing::info!("Stopping receiver...");
    receiver.stop().await?;

    // 5. Wait for Disconnected event
    tracing::info!("Waiting for Disconnected event...");

    let event = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Disconnected { reason, .. }) => {
                    tracing::info!("Received Disconnected event: {}", reason);
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
            tracing::info!("✓ Disconnection detected successfully");
            assert!(
                !client.is_connected().await,
                "Client should report disconnected"
            );
            Ok(())
        }
        Ok(Err(e)) => Err(format!("Event receiver error: {}", e).into()),
        Err(_) => Err("Timeout waiting for Disconnected event (15s)".into()),
    }
}

#[tokio::test]
async fn test_automatic_reconnection() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Automatic Reconnection test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();
    let initial_id = device.id.clone();
    tracing::info!("Receiver started with ID: {}", initial_id);
    sleep(Duration::from_secs(3)).await;

    // 2. Connect Player with auto-reconnect enabled
    tracing::info!("Connecting player...");
    let config = AirPlayConfig::builder()
        .pin("3939")
        .build();

    let mut player = AirPlayPlayer::with_config(config);
    player.set_auto_reconnect(true);

    // Direct connect to avoid mDNS flakiness on initial connect
    if let Err(e) = player.connect(&device).await {
        return Err(e.into());
    }
    assert!(player.is_connected().await);

    // 3. Kill Receiver
    tracing::info!("Stopping receiver...");
    receiver.stop().await?;

    // 4. Wait for disconnection detection
    tracing::info!("Waiting for disconnection...");
    sleep(Duration::from_secs(5)).await;
    assert!(
        !player.is_connected().await,
        "Player should be disconnected"
    );

    // 5. Restart Receiver (simulating recovery)
    tracing::info!("Restarting receiver...");
    // Start new receiver. It should have same ID if using loopback MAC.
    // We cannot easily force ID here, but we hope.
    let _receiver2 = PythonReceiver::start().await?;
    sleep(Duration::from_secs(3)).await; // Allow it to announce

    // Verify ID matches (just for debug)
    let device2 = _receiver2.device_config();
    tracing::info!("New receiver ID: {}", device2.id);
    if device2.id != initial_id {
        tracing::warn!("Receiver ID changed from {} to {}! Auto-reconnect might fail if it relies on ID.", initial_id, device2.id);
        // Note: If ID changes, the test WILL fail because client looks for old ID.
        // But let's proceed and see.
    }

    // 6. Wait for Reconnection
    tracing::info!("Waiting for reconnection (max 30s)...");
    let reconnected = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if player.is_connected().await {
                // Verify we are connected to the new device (check port?)
                let connected_device = player.device().await;
                if let Some(d) = connected_device {
                     tracing::info!("Reconnected to: {} (Port: {})", d.name, d.port);
                     break;
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
    })
    .await;

    match reconnected {
        Ok(()) => {
            tracing::info!("✓ Automatic reconnection successful");
            Ok(())
        }
        Err(_) => Err("Timeout waiting for automatic reconnection".into()),
    }
}

#[tokio::test]
async fn test_no_reconnect_on_user_disconnect() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting No-Reconnect on User Disconnect test");

    // 1. Start Receiver
    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();
    sleep(Duration::from_secs(3)).await;

    // 2. Connect Player with auto-reconnect enabled
    let config = AirPlayConfig::builder()
        .pin("3939")
        .build();

    let mut player = AirPlayPlayer::with_config(config);
    player.set_auto_reconnect(true);

    player.connect(&device).await?;
    assert!(player.is_connected().await);

    // 3. User Disconnect
    tracing::info!("User disconnecting...");
    player.disconnect().await?;
    assert!(!player.is_connected().await);

    // 4. Wait and Verify NO reconnection
    tracing::info!("Waiting to ensure no reconnection happens...");
    sleep(Duration::from_secs(5)).await;

    assert!(!player.is_connected().await, "Player should remain disconnected after user disconnect");
    Ok(())
}
