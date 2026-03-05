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
            std::env::set_var("RUST_LOG", "info");
        }
    }
    tracing_subscriber::fmt::init();

    let target_name = "Kitchen";
    println!("Connecting to '{}'...", target_name);

    #[allow(unused_mut)]
    let mut player = AirPlayPlayer::new();
    let mut retry_count = 0;
    let max_retries = 5;

    loop {
        match player
            .connect_by_name(target_name, Duration::from_secs(3))
            .await
        {
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
                    println!(
                        "Could not find '{}'. Attempting auto-connect to any device...",
                        target_name
                    );
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

    // --- PTP Sync Verification ---
    println!("\n=== Checking PTP timing status ===");
    let client = player.client().clone();
    if client.is_ptp_active().await {
        println!("PTP is active. Waiting for clock synchronization...");
        let mut ptp_synced = false;
        for attempt in 0..20 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Some((synced, offset_ms, measurements)) = client.ptp_status().await {
                println!(
                    "  PTP [{}/20]: synced={}, offset={:.3}ms, measurements={}",
                    attempt + 1,
                    synced,
                    offset_ms,
                    measurements
                );
                if synced {
                    println!(
                        "✓ PTP synchronized! offset={:.3}ms after {} measurements",
                        offset_ms, measurements
                    );
                    ptp_synced = true;
                    break;
                }
            } else {
                println!("  PTP [{}/20]: no status available yet", attempt + 1);
            }
        }
        if !ptp_synced {
            eprintln!("✗ WARNING: PTP did not synchronize within 5 seconds!");
            eprintln!("  Audio may not play correctly without clock sync.");
        }
    } else {
        println!("PTP is not active (device may use NTP or another timing method).");
    }
    println!("=================================\n");

    let file_path = "Eels - 01 - Susan's House.mp3";
    println!("Playing file: {}", file_path);

    // Stop previous playback if any
    let _ = player.stop().await;

    #[cfg(feature = "decoders")]
    {
        println!("Starting playback...");

        // Clone client for monitoring (AirPlayPlayer is not Clone, but Client is)
        let monitor_client = player.client().clone();

        // Spawn playback in a separate task
        let play_task = tokio::spawn(async move { player.play_file(file_path).await });

        // Wait for playback to start
        tokio::time::sleep(Duration::from_secs(2)).await;

        // --- Verify playback is active ---
        println!("\n=== Verifying playback state ===");
        // Check local state
        let state = monitor_client.playback_state().await;
        println!(
            "Local playback state: playing={}, position={:.1}s",
            state.is_playing, state.position_secs
        );

        // Check PTP sync status during playback
        if let Some((synced, offset_ms, measurements)) = monitor_client.ptp_status().await {
            println!(
                "PTP during playback: synced={}, offset={:.3}ms, measurements={}",
                synced, offset_ms, measurements
            );
            if !synced {
                eprintln!("✗ WARNING: PTP still not synchronized during playback!");
            } else {
                println!("✓ PTP is synchronized during playback.");
            }
        }

        // Try to get playback info from device
        println!("Querying device playback status...");
        match monitor_client.get_playback_info().await {
            Ok(info_bytes) if !info_bytes.is_empty() => {
                println!("Device playback info ({} bytes):", info_bytes.len());
                if let Ok(s) = String::from_utf8(info_bytes.clone()) {
                    println!("{}", s.trim());
                } else {
                    let display_len = std::cmp::min(info_bytes.len(), 64);
                    println!("(binary) {:02X?}...", &info_bytes[..display_len]);
                }
            }
            Ok(_) => println!("Device returned empty playback info."),
            Err(e) => println!("Could not get device playback info: {}", e),
        }

        println!("================================\n");

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
