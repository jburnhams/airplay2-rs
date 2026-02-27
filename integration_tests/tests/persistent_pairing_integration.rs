//! Integration tests for persistent pairing
//!
//! Verifies that we can pair once, disconnect, and reconnect using stored keys (Pair-Verify)
//! without needing to Pair-Setup again.

use std::time::Duration;

use tokio::time::sleep;

mod common;
use airplay2::AirPlayClient;
use airplay2::protocol::pairing::storage::FileStorage;
use common::python_receiver::PythonReceiver;

/// Helper to connect with retry logic
async fn connect_with_retry(
    client: &mut AirPlayClient,
    device: &airplay2::AirPlayDevice,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut connected = false;
    for i in 0..3 {
        tracing::info!("Connection attempt {}/3...", i + 1);
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
async fn test_persistent_pairing_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting Persistent Pairing integration test");

    // 1. Setup paths
    let temp_dir = std::env::temp_dir();
    let storage_path = temp_dir.join(format!(
        "airplay2_test_storage_{}.json",
        chrono::Utc::now().timestamp_millis()
    ));

    // Ensure storage file doesn't exist
    if storage_path.exists() {
        std::fs::remove_file(&storage_path)?;
    }

    // 2. Start Receiver
    // Note: PythonReceiver::start() cleans up the receiver's pairings directory
    let receiver = PythonReceiver::start().await?;

    // Give receiver time to start
    sleep(Duration::from_secs(2)).await;
    let device = receiver.device_config();

    // 3. Connect Client A (Initial Pairing)
    tracing::info!("--- Step 1: Initial Pairing (Pair-Setup) ---");
    {
        let storage = FileStorage::new(&storage_path).await?;
        let config = airplay2::AirPlayConfig::builder()
            .pairing_storage(storage_path.clone()) // This sets the path in config, but we also pass storage directly
            .pin("3939")
            .build();

        // We use with_pairing_storage to inject the storage instance
        let mut client = AirPlayClient::new(config).with_pairing_storage(Box::new(storage));

        connect_with_retry(&mut client, &device).await?;
        assert!(client.is_connected().await, "Client A should be connected");

        // Wait a bit to ensure keys are saved and receiver flushes pairing to disk.
        // Python receiver might take some time to write the file.
        sleep(Duration::from_secs(2)).await;

        client.disconnect().await?;
    }

    // 4. Verify storage file created
    assert!(
        storage_path.exists(),
        "Storage file should exist after pairing"
    );
    let content = std::fs::read_to_string(&storage_path)?;
    tracing::info!("Storage content: {}", content);
    assert!(
        content.contains(&device.id),
        "Storage should contain device ID '{}'",
        device.id
    );

    // 5. Connect Client B (Reconnect with Pair-Verify)
    tracing::info!("--- Step 2: Reconnection (Pair-Verify) ---");
    {
        // New client instance, but loading from same storage
        let storage = FileStorage::new(&storage_path).await?;

        // Verify we actually loaded something
        use airplay2::protocol::pairing::storage::PairingStorage;
        let keys = storage.load(&device.id).await;
        assert!(
            keys.is_some(),
            "Should have loaded keys for device {}",
            device.id
        );

        let config = airplay2::AirPlayConfig::builder()
            .pairing_storage(storage_path.clone())
            .pin("3939")
            .build();

        let mut client = AirPlayClient::new(config).with_pairing_storage(Box::new(storage));

        // This connect() call should use Pair-Verify because keys exist in storage
        // and the receiver should recognize us.
        connect_with_retry(&mut client, &device).await?;

        assert!(
            client.is_connected().await,
            "Client B should be connected via persistent pairing"
        );

        // Verify we can do something authenticated, e.g. set volume
        client.set_volume(0.5).await?;

        client.disconnect().await?;
    }

    // 6. Cleanup
    let _ = receiver.stop().await?;
    if storage_path.exists() {
        std::fs::remove_file(storage_path)?;
    }

    tracing::info!("✅ Persistent Pairing integration test passed");
    Ok(())
}
