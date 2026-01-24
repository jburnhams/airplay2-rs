//! Example: Discover AirPlay devices on the network

use airplay2::{AirPlayDevice, scan};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Scanning for AirPlay devices (5 seconds)...");

    // Scan for devices
    let devices = scan(Duration::from_secs(5)).await?;

    if devices.is_empty() {
        println!("No devices found.");
        return Ok(());
    }

    println!("Found {} devices:", devices.len());
    for (i, device) in devices.iter().enumerate() {
        print_device_info(i + 1, device);
    }

    Ok(())
}

fn print_device_info(index: usize, device: &AirPlayDevice) {
    println!("\nDevice #{}: {}", index, device.name);
    println!("  ID: {}", device.id);
    println!("  Address: {}:{}", device.address, device.port);

    if let Some(model) = &device.model {
        println!("  Model: {}", model);
    }

    println!("  Capabilities:");
    println!("    - AirPlay 2: {}", device.supports_airplay2());
    println!("    - Multi-room: {}", device.supports_grouping());

    if let Some(vol) = device.discovered_volume() {
        println!("    - Volume: {:.0}%", vol * 100.0);
    }
}
