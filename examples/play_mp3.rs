//! Example: Play MP3 file to "Kitchen"
//!
//! Run with: `cargo run --example play_mp3 --features decoders`

use airplay2::AirPlayPlayer;
use std::time::Duration;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "debug");
        }
    }
    tracing_subscriber::fmt::init();

    let target_name = "Kitchen";
    println!("Connecting to '{}'...", target_name);

    let player = AirPlayPlayer::new();
    let mut retry_count = 0;
    let max_retries = 5;

    loop {
        match player.connect_by_name(target_name, Duration::from_secs(3)).await {
            Ok(_) => {
                println!("Connected successfully to '{}'!", target_name);
                break;
            }
            Err(e) => {
                eprintln!("Failed to connect: {}", e);
                
                // Scan and list available devices to help debugging
                println!("Scanning for devices...");
                match player.client().scan(Duration::from_secs(2)).await {
                    Ok(devices) => {
                        println!("Found {} devices:", devices.len());
                        for d in devices {
                            println!(" - '{}' ({:?}:{})", d.name, d.addresses.first(), d.port);
                        }
                    }
                    Err(_) => println!("Scan failed."),
                }

                retry_count += 1;
                if retry_count >= max_retries {
                    println!("Could not find '{}'. Attempting auto-connect to any device...", target_name);
                    player.auto_connect(Duration::from_secs(5)).await?;
                    if let Some(device) = player.device().await {
                         println!("Connected to '{}'!", device.name);
                    }
                    break;
                }
                println!("Retrying in 2 seconds...");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    let file_path = "Eels - 01 - Susan's House.mp3";
    println!("Playing file: {}", file_path);

    // Stop previous playback if any
    let _ = player.stop().await;

    // Play file (this blocks until playback is finished or error)
    // Note: stream_audio currently blocks the caller in the loop.
    // In a real player, we might want to spawn this.
    // But for this example, blocking is fine.
    
    // We need to use a separate task to handle Ctrl+C or user input if we want to stop early?
    // Actually stream_audio returns when EOF.
    
    
    #[cfg(feature = "decoders")]
    {
        println!("Starting playback...");
        println!("Note: Setting volume to 25% (-12dB) after playback starts to work around HomePod 455 error.");
        
        // Clone client for volume control (AirPlayPlayer is not Clone, but Client is)
        let volume_client = player.client().clone();
        
        // Spawn playback in a separate task
        // We move player into the task
        let play_task = tokio::spawn(async move {
            player.play_file(file_path).await
        });

        // Wait for playback to likely have started (RTSP negotiation takes ~1-2s)
        tokio::time::sleep(Duration::from_secs(3)).await;
        
        // Attempt to set volume with retries
        println!("Setting volume...");
        let mut volume_set = false;
        for i in 0..5 {
            match volume_client.set_volume(0.25).await {
                Ok(_) => {
                    println!("Volume set successfully.");
                    volume_set = true;
                    break;
                }
                Err(e) => {
                    eprintln!("Failed to set volume (attempt {}/5): {}", i + 1, e);
                    if i < 4 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
        
        if !volume_set {
             eprintln!("Warning: Could not set volume after multiple attempts. Audio might be silent.");
        }

        // Query playback info to confirm device state difference (Debug only)
        /*
        println!("Querying device playback status...");
        match volume_client.get_playback_info().await {
            Ok(info_bytes) => {
                println!("Playback Info Response ({} bytes):", info_bytes.len());
                // Try to parse as string, otherwise hex
                if let Ok(s) = String::from_utf8(info_bytes.clone()) {
                    println!("{}", s.trim());
                } else {
                    // Truncate long binary data for display
                    let display_len = std::cmp::min(info_bytes.len(), 64);
                    println!("{:02X?}...", &info_bytes[..display_len]);
                }
            }
            Err(e) => eprintln!("Failed to get playback info: {}", e),
        }
        */

        // Wait for playback to finish
        match play_task.await {
            Ok(Ok(_)) => println!("Playback finished successfully."),
            Ok(Err(e)) => eprintln!("Playback error: {}", e),
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    println!("\nPlayback finished.");
    Ok(())
}
