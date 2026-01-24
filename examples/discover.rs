//! Device discovery example

use airplay2::scan;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("Discovering AirPlay devices...");

    // Scan for longer
    let devices = scan(Duration::from_secs(5)).await?;

    if devices.is_empty() {
        println!("No devices found.");
    } else {
        println!("Found {} devices:", devices.len());
        for device in devices {
            println!("  - {} ({}): {:?}", device.name, device.id, device.address);
        }
    }
    Ok(())
}
