//! Integration test for reconnection logic

use std::time::Duration;
use tokio::time::sleep;

mod common;
use airplay2::state::ClientEvent;
use airplay2::{AirPlayClient, AirPlayConfig, AirPlayPlayer};
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_disconnection_detection() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Disconnection Detection test");

    let receiver = PythonReceiver::start().await?;
    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    tracing::info!("Connecting client...");
    let config = AirPlayConfig::builder()
        .pin("3939")
        .connection_timeout(Duration::from_secs(5))
        .build();

    let client = AirPlayClient::new(config);
    client.connect(&device).await?;
    assert!(client.is_connected().await, "Client should be connected");

    let mut rx = client.subscribe_events();

    tracing::info!("Stopping receiver...");
    receiver.stop().await?;

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
        .connection_timeout(Duration::from_secs(5))
        .build();

    let mut player = AirPlayPlayer::with_config(config);
    player.set_auto_reconnect(true);

    // Subscribe to events BEFORE connecting to catch everything
    let mut rx = player.client().subscribe_events();

    player.connect(&device).await?;
    assert!(player.is_connected().await);

    // 3. Stop Receiver
    tracing::info!("Stopping receiver...");
    receiver.stop().await?;

    // 4. Wait for Disconnected event
    tracing::info!("Waiting for Disconnected event...");
    let disconnected = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Disconnected { reason, .. }) => {
                    tracing::info!("Disconnected: {}", reason);
                    return;
                }
                _ => {}
            }
        }
    })
    .await;

    assert!(disconnected.is_ok(), "Timeout waiting for disconnect");
    assert!(
        !player.is_connected().await,
        "Player should be disconnected"
    );

    // 5. Restart Receiver (simulating recovery)
    tracing::info!("Restarting receiver...");
    let _receiver2 = PythonReceiver::start().await?;
    // Wait slightly for mDNS announcement
    sleep(Duration::from_secs(2)).await;

    // 6. Wait for Reconnected event
    tracing::info!("Waiting for Reconnected event (max 30s)...");
    let reconnected = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Connected { device }) => {
                    tracing::info!("Reconnected to: {}", device.name);
                    return Ok(());
                }
                Ok(e) => tracing::debug!("Ignored event during reconnect: {:?}", e),
                Err(e) => return Err(format!("Recv error: {}", e)),
            }
        }
    })
    .await;

    match reconnected {
        Ok(Ok(())) => {
            tracing::info!("✓ Automatic reconnection successful");
            assert!(player.is_connected().await);
            Ok(())
        }
        Ok(Err(e)) => Err(format!("Event receiver error: {}", e).into()),
        Err(_) => Err("Timeout waiting for automatic reconnection".into()),
    }
}

#[tokio::test]
async fn test_no_reconnect_on_user_disconnect() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting No-Reconnect on User Disconnect test");

    let receiver = PythonReceiver::start().await?;
    let device = receiver.device_config();
    sleep(Duration::from_secs(2)).await;

    let config = AirPlayConfig::builder()
        .pin("3939")
        .connection_timeout(Duration::from_secs(5))
        .build();

    let mut player = AirPlayPlayer::with_config(config);
    player.set_auto_reconnect(true);

    let mut rx = player.client().subscribe_events();

    player.connect(&device).await?;
    assert!(player.is_connected().await);

    // User Disconnect
    tracing::info!("User disconnecting...");
    player.disconnect().await?;

    // Wait for Disconnected event
    let disconnected = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Disconnected { reason, .. }) => {
                    tracing::info!("Disconnected: {}", reason);
                    if reason.contains("UserRequested") {
                        return;
                    }
                }
                _ => {}
            }
        }
    })
    .await;

    assert!(disconnected.is_ok(), "Timeout waiting for user disconnect");
    assert!(!player.is_connected().await);

    // Ensure no reconnection happens
    tracing::info!("Waiting to ensure no reconnection happens...");
    let unexpected_reconnect = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(ClientEvent::Connected { .. }) => return true,
                _ => {}
            }
        }
    })
    .await;

    assert!(
        unexpected_reconnect.is_err(),
        "Unexpected reconnection occurred!"
    );
    assert!(
        !player.is_connected().await,
        "Player should remain disconnected"
    );

    Ok(())
}
