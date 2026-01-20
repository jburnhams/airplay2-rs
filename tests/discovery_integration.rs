#[tokio::test]
#[ignore] // Run manually with `cargo test -- --ignored`
async fn test_discover_real_devices() {
    use airplay2::scan;
    use std::time::Duration;

    let devices = scan(Duration::from_secs(5)).await.unwrap();

    println!("Found {} devices:", devices.len());
    for device in &devices {
        println!("  - {} ({})", device.name, device.id);
        println!("    Address: {}", device.address);
        println!("    Port: {}", device.port);
        println!("    Model: {:?}", device.model);
        println!("    AirPlay 2: {}", device.supports_airplay2());
        println!("    Grouping: {}", device.supports_grouping());
    }

    // At least verify we can run without crashing
}
