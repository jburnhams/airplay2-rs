//! Example: Play a URL on an AirPlay device

use airplay2::quick_connect;
use tokio::io::{self, AsyncBufReadExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to first available device...");

    // Quick connect helper
    let player = quick_connect().await?;

    if let Some(device) = player.device().await {
        println!("Connected to: {}", device.name);
    }

    // Play a sample track
    let url =
        "http://commondatastorage.googleapis.com/codeskulptor-demos/riceracer_assets/music/win.ogg";
    println!("Playing: {}", url);

    // Fixed: Added url as first argument
    player
        .play_track(url, "Winning Sound", "Demo Artist")
        .await?;

    // Wait for user input to stop
    println!("\nPress Enter to stop...");
    let mut stdin = io::BufReader::new(io::stdin());
    let mut line = String::new();
    stdin.read_line(&mut line).await?;

    println!("Stopping...");
    player.stop().await?;
    player.disconnect().await?;

    Ok(())
}
