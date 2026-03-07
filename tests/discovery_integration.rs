#[tokio::test]
async fn test_discover_real_devices() {
    use std::time::Duration;

    use airplay2::scan;

    let devices = scan(Duration::from_secs(5)).await.unwrap();

    println!("Found {} devices:", devices.len());
    for device in &devices {
        println!("  - {} ({})", device.name, device.id);
        println!("    Address: {}", device.address());
        println!("    Port: {}", device.port);
        println!("    Model: {:?}", device.model);
        println!("    AirPlay 2: {}", device.supports_airplay2());
        println!("    Grouping: {}", device.supports_grouping());
    }

    // At least verify we can run without crashing
}

#[tokio::test]
async fn test_discover_timeout_handling() {
    use std::time::Duration;

    use airplay2::scan;

    // Use a very short timeout to test that discovery times out gracefully
    let devices = scan(Duration::from_millis(1)).await;

    // The scan function shouldn't error, just return whatever it found (likely an empty list, but
    // we mainly care it doesn't panic)
    assert!(devices.is_ok());
    let devices_list = devices.unwrap();

    // We expect it to be empty because 1ms is generally not enough for mDNS discovery,
    // but the main point is it didn't crash.
    // However, we won't assert it is strictly empty in case of some weird OS caching,
    // just asserting it returned successfully is enough.
    println!(
        "Scan with 1ms timeout returned {} devices",
        devices_list.len()
    );
}
