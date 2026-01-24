//! Example: Interactive CLI player

use airplay2::quick_connect;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting...");
    let player = quick_connect().await?;

    if let Some(device) = player.device().await {
        println!("Connected to: {}", device.name);
    }

    println!("\nCommands:");
    println!("  play <url>    - Play URL");
    println!("  pause         - Pause playback");
    println!("  resume        - Resume playback");
    println!("  stop          - Stop playback");
    println!("  vol <0-100>   - Set volume");
    println!("  quit          - Exit");

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "play" if parts.len() > 1 => {
                let url = parts[1];
                println!("Playing: {}", url);
                // Fixed: Added url as first argument
                player.play_track(url, "Stream", "User").await?;
            }
            "pause" => {
                player.pause().await?;
                println!("Paused");
            }
            "resume" => {
                player.play().await?;
                println!("Resumed");
            }
            "stop" => {
                player.stop().await?;
                println!("Stopped");
            }
            "vol" if parts.len() > 1 => {
                if let Ok(vol) = parts[1].parse::<u8>() {
                    player.set_volume(f32::from(vol) / 100.0).await?;
                    println!("Volume: {}%", vol);
                }
            }
            "quit" => break,
            _ => println!("Unknown command"),
        }
    }

    player.disconnect().await?;
    Ok(())
}
