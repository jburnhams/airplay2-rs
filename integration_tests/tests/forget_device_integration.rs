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

    let mut config = AirPlayConfig::default();
    config.discovery_timeout = Duration::from_secs(5);
    config.connection_timeout = Duration::from_secs(10); // Increase timeout
    config.pin = Some("3939".to_string());

    // 3. Connect and Pair with retry
    let device = receiver.device_config();
    let mut last_error = None;

    // We need to declare client outside the loop to use it later
    // But since `with_pairing_storage` consumes `self`, we need to recreate it if we fail
    // However, `AirPlayClient` is not Clone if it has custom storage...
    // But here we create a fresh client each loop.

    // Since we need the client AFTER the loop, we'll assign it to an Option
    let mut final_client = None;

    for i in 1..=5 {
        // Re-create storage for each attempt (FileStorage handles existing files fine)
        let storage = FileStorage::new(&storage_path)
            .await
            .expect("Failed to create storage");

        let client = AirPlayClient::new(config.clone()).with_pairing_storage(Box::new(storage));

        println!("Connection attempt {}/5...", i);

        // Add delay before attempt
        tokio::time::sleep(Duration::from_secs(1)).await;

        match client.connect(&device).await {
            Ok(_) => {
                final_client = Some(client);
                break;
            }
            Err(e) => {
                println!("Connection attempt {} failed: {}", i, e);
                last_error = Some(e);
                if i < 5 {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    let client = final_client.expect(&format!("Failed to connect after 5 attempts. Last error: {:?}", last_error));

    // Verify keys are stored
    // We can't access client's storage directly easily.
    // But we can create another FileStorage instance to check the file.
    {
        let check_storage = FileStorage::new(&storage_path)
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
        let check_storage = FileStorage::new(&storage_path)
            .await
            .expect("Failed to open storage");
        let keys = check_storage.load(&device.id).await;
        assert!(keys.is_none(), "Keys should be removed after forget_device");
    }
}
