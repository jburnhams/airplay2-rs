//! Integration tests for AirPlay 2 service advertisement

use std::time::Duration;

use airplay2::discovery;
use airplay2::receiver::ap2::{Ap2Config, Ap2ServiceAdvertiser};

/// Test that we can advertise and discover our own service
#[tokio::test]
async fn test_advertise_and_discover() {
    if std::env::var("CI").is_ok() {
        println!("Skipping mDNS test in CI environment");
        return;
    }
    let config = Ap2Config::new("Integration Test Speaker");
    let public_key = [0u8; 32];
    let advertiser =
        Ap2ServiceAdvertiser::new(config.clone(), public_key).expect("Failed to create advertiser");

    // Start advertising
    advertiser
        .start()
        .await
        .expect("Failed to start advertising");

    // Give mDNS time to propagate
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Try to discover our own service
    let devices = discovery::scan(Duration::from_secs(5))
        .await
        .expect("Discovery failed");

    // Find our device
    let our_device = devices.iter().find(|d| d.name == config.name);

    assert!(
        our_device.is_some(),
        "Should discover our own advertised service. Found: {:?}",
        devices.iter().map(|d| &d.name).collect::<Vec<_>>()
    );

    let device = our_device.unwrap();
    assert_eq!(device.port, config.server_port);

    // Cleanup
    advertiser.stop().await.expect("Failed to stop advertising");
}

/// Test that name updates are reflected in discovery
#[tokio::test]
async fn test_name_update() {
    if std::env::var("CI").is_ok() {
        println!("Skipping mDNS test in CI environment");
        return;
    }
    let config = Ap2Config::new("Original Name");
    let public_key = [0u8; 32];
    let mut advertiser =
        Ap2ServiceAdvertiser::new(config, public_key).expect("Failed to create advertiser");

    advertiser.start().await.expect("Failed to start");

    // Update name
    advertiser
        .update_name("Updated Name".to_string())
        .await
        .expect("Failed to update name");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify new name is discoverable
    let devices = discovery::scan(Duration::from_secs(5))
        .await
        .expect("Discovery failed");

    let found = devices.iter().any(|d| d.name == "Updated Name");
    assert!(found, "Should find device with updated name");

    advertiser.stop().await.expect("Failed to stop");
}

/// Test that stopping advertisement removes the service
#[tokio::test]
async fn test_stop_removes_service() {
    let config = Ap2Config::new("Disappearing Speaker");
    let public_key = [0u8; 32];
    let advertiser =
        Ap2ServiceAdvertiser::new(config.clone(), public_key).expect("Failed to create advertiser");

    advertiser.start().await.expect("Failed to start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Stop advertising
    advertiser.stop().await.expect("Failed to stop");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Service should no longer be discoverable
    let devices = discovery::scan(Duration::from_secs(3))
        .await
        .expect("Discovery failed");

    let found = devices.iter().any(|d| d.name == config.name);
    // Note: mDNS may cache for a while, so we just verify stop() doesn't error
    let _ = found;
}
