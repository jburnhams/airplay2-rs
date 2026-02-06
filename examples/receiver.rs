//! Simple AirPlay receiver example

use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    tracing_subscriber::fmt::init();

    // Create receiver with custom name
    let config = ReceiverConfig::with_name("Rust AirPlay Receiver").latency_ms(2000);

    let mut receiver = AirPlayReceiver::new(config);

    // Handle events
    let mut events = receiver.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                ReceiverEvent::Started { name, port } => {
                    println!("Receiver '{}' started on port {}", name, port);
                }
                ReceiverEvent::ClientConnected { address, .. } => {
                    println!("Client connected from {}", address);
                }
                ReceiverEvent::PlaybackStarted => {
                    println!("Playback started!");
                }
                ReceiverEvent::VolumeChanged { linear, muted, .. } => {
                    if muted {
                        println!("Muted");
                    } else {
                        println!("Volume: {:.0}%", linear * 100.0);
                    }
                }
                ReceiverEvent::MetadataUpdated(meta) => {
                    if let (Some(title), Some(artist)) = (&meta.title, &meta.artist) {
                        println!("Now playing: {} - {}", artist, title);
                    }
                }
                ReceiverEvent::Stopped => {
                    println!("Receiver stopped.");
                    break;
                }
                _ => {}
            }
        }
    });

    // Start receiver
    receiver.start().await?;
    println!("Receiver running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    // Cleanup
    receiver.stop().await?;
    println!("Receiver stopped.");

    Ok(())
}
