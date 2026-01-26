//! Integration tests for RAOP service advertisement
//!
//! These tests verify that advertised services can be discovered
//! by the existing browser functionality.

use airplay2::discovery::{
    advertiser::{AdvertiserConfig, AsyncRaopAdvertiser, ReceiverStatusFlags},
    scan_with_options, DiscoveryOptions,
};
use std::time::Duration;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();
}

/// Test that an advertised service can be discovered
#[tokio::test]
#[ignore]
async fn test_advertise_and_discover() {
    init_tracing();
    // Start advertiser
    let config = AdvertiserConfig {
        name: "Test Receiver".to_string(),
        port: 15000, // Use high port to avoid conflicts
        mac_override: Some([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01]),
        ..Default::default()
    };

    let advertiser = AsyncRaopAdvertiser::start(config).unwrap();

    // Wait for advertisement to propagate
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Browse for services
    let options = DiscoveryOptions {
        discover_airplay2: false,
        discover_raop: true,
        timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let services = scan_with_options(options).await.unwrap();

    // Find our service
    let found = services.iter().find(|s| s.name.contains("Test Receiver"));
    assert!(found.is_some(), "Service should be discoverable");

    let service = found.unwrap();
    // Note: AirPlayDevice port might be the main port, which for RAOP-only devices is the RAOP port.
    // The browser logic sets device.port = info.get_port() if only RAOP.
    assert_eq!(service.port, 15000);

    // Cleanup
    advertiser.shutdown().await;
}

/// Test status update visibility
#[tokio::test]
#[ignore]
async fn test_status_update_reflected_in_txt() {
    let config = AdvertiserConfig {
        name: "Status Test".to_string(),
        port: 15001,
        mac_override: Some([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x02]),
        ..Default::default()
    };

    let advertiser = AsyncRaopAdvertiser::start(config).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Update status to busy
    advertiser
        .update_status(ReceiverStatusFlags {
            busy: true,
            ..Default::default()
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Browse and check TXT record
    let options = DiscoveryOptions {
        discover_airplay2: false,
        discover_raop: true,
        timeout: Duration::from_secs(3),
        ..Default::default()
    };

    let services = scan_with_options(options).await.unwrap();

    let found = services.iter().find(|s| s.name.contains("Status Test"));
    if let Some(service) = found {
        let sf = service
            .txt_records
            .get("sf")
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
        assert_eq!(sf, Some(0x04), "Busy flag should be set");
    }

    advertiser.shutdown().await;
}

/// Test multiple advertisers with different names
#[tokio::test]
#[ignore]
async fn test_multiple_advertisers() {
    let configs = [
        AdvertiserConfig {
            name: "Kitchen".to_string(),
            port: 15010,
            mac_override: Some([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]),
            ..Default::default()
        },
        AdvertiserConfig {
            name: "Bedroom".to_string(),
            port: 15011,
            mac_override: Some([0x01, 0x02, 0x03, 0x04, 0x05, 0x07]),
            ..Default::default()
        },
    ];

    let mut advertisers = Vec::new();
    for config in configs {
        advertisers.push(AsyncRaopAdvertiser::start(config).unwrap());
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    let options = DiscoveryOptions {
        discover_airplay2: false,
        discover_raop: true,
        timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let services = scan_with_options(options).await.unwrap();

    assert!(services.iter().any(|s| s.name.contains("Kitchen")));
    assert!(services.iter().any(|s| s.name.contains("Bedroom")));

    for advertiser in advertisers {
        advertiser.shutdown().await;
    }
}

/// Test graceful shutdown removes service
#[tokio::test]
#[ignore]
async fn test_shutdown_removes_service() {
    let config = AdvertiserConfig {
        name: "Temporary".to_string(),
        port: 15020,
        mac_override: Some([0xFE, 0xED, 0xFA, 0xCE, 0x00, 0x01]),
        ..Default::default()
    };

    let advertiser = AsyncRaopAdvertiser::start(config).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify visible
    let options = DiscoveryOptions {
        discover_airplay2: false,
        discover_raop: true,
        timeout: Duration::from_secs(3),
        ..Default::default()
    };
    let services = scan_with_options(options.clone()).await.unwrap();
    assert!(services.iter().any(|s| s.name.contains("Temporary")));

    // Shutdown
    advertiser.shutdown().await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify gone (may take a moment for mDNS to propagate)
    // Note: scan waits for timeout.
    let services = scan_with_options(options).await.unwrap();
    assert!(
        !services.iter().any(|s| s.name.contains("Temporary")),
        "Service should be gone"
    );
}
