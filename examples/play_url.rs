//! URL playback example

use airplay2::{AirPlayClient, AirPlayConfig, scan};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging (optional)
    tracing_subscriber::fmt::init();

    println!("Scanning for AirPlay devices...");
    let devices = scan(Duration::from_secs(2)).await?;

    if devices.is_empty() {
        println!("No devices found.");
        return Ok(());
    }

    println!("Found {} devices.", devices.len());
    let device = &devices[0];
    println!("Connecting to: {} ({})", device.name, device.address);

    let client = AirPlayClient::new(AirPlayConfig::default());
    client.connect(device).await?;
    println!("Connected.");

    let url = "http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";
    println!("Playing URL: {}", url);

    match client.play_url(url).await {
        Ok(_) => {
            println!("Playback started. Waiting for 10 seconds...");
            tokio::time::sleep(Duration::from_secs(10)).await;

            println!("Stopping...");
            client.stop().await?;
        }
        Err(e) => {
            println!("Failed to play: {}", e);
        }
    }

    println!("Disconnecting...");
    client.disconnect().await?;

    Ok(())
}
