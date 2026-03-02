use std::time::Duration;

use airplay2::protocol::pairing::PairingStorage;
use airplay2::protocol::pairing::storage::FileStorage;
use airplay2::{AirPlayClient, AirPlayConfig};
use tempfile::tempdir;

use crate::common::python_receiver::PythonReceiver;

mod common;

#[tokio::test]
async fn test_forget_device() {
    // 1. Start receiver
    let receiver = PythonReceiver::start()
        .await
        .expect("Failed to start receiver");

    // Give receiver time to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 2. Setup persistent storage
    let dir = tempdir().expect("Failed to create temp dir");
    let storage_path = dir.path().join("pairings.json");

    let config = AirPlayConfig {
        discovery_timeout: Duration::from_secs(5),
        connection_timeout: Duration::from_secs(5),
        pin: Some("3939".to_string()),
        ..Default::default()
    };
    // Note: setting config.pairing_storage_path is not enough because AirPlayClient::new(config)
    // initializes with NO storage (MemoryStorage).
    // We MUST use with_pairing_storage().

    let storage = FileStorage::new(&storage_path, None)
        .await
        .expect("Failed to create storage");

    let client = AirPlayClient::new(config.clone()).with_pairing_storage(Box::new(storage));

    // 3. Connect and Pair
    let device = receiver.device_config();

    // Use retry logic for robustness in CI environments
    let mut connected = false;
    for i in 0..3 {
        println!("Connection attempt {}/3...", i + 1);
        if client.connect(&device).await.is_ok() {
            connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    if !connected {
        panic!("Failed to connect client after retries");
    }

    // Verify keys are stored
    // We can't access client's storage directly easily.
    // But we can create another FileStorage instance to check the file.
    {
        let check_storage = FileStorage::new(&storage_path, None)
            .await
            .expect("Failed to open storage");
        let keys = check_storage.load(&device.id).await;
        assert!(keys.is_some(), "Keys should be stored after connection");
    }

    // 4. Disconnect
    client.disconnect().await.expect("Failed to disconnect");

    // 5. Forget device
    client
        .forget_device(&device.id)
        .await
        .expect("Failed to forget device");

    // 6. Verify keys are removed
    {
        let check_storage = FileStorage::new(&storage_path, None)
            .await
            .expect("Failed to open storage");
        let keys = check_storage.load(&device.id).await;
        assert!(keys.is_none(), "Keys should be removed after forget_device");
    }
}
