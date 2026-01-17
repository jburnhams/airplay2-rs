//! Device discovery example

use airplay2::scan;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Discovering AirPlay devices (requires Section 08 implementation)...");

    // This will currently return an empty list as discovery is not implemented
    let devices = scan(Duration::from_secs(2)).await?;

    if devices.is_empty() {
        println!("No devices found (Discovery module stubbed).");
    } else {
        for device in devices {
            println!("Found: {:?}", device);
        }
    }
    Ok(())
}
