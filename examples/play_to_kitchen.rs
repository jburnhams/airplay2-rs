//! Example: Connect to "Kitchen" and play audio
//!
//! This example demonstrates how to:
//! 1. Discover a specific device by name ("Kitchen")
//! 2. Connect using transient pairing (default)
//! 3. Stream PCM audio (Sine Wave)

use airplay2::streaming::source::SliceSource;
use airplay2::{AirPlayPlayer, audio::AudioFormat};

use tokio::io::{self, AsyncBufReadExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging to see what's happening
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "debug");
        }
    }
    tracing_subscriber::fmt::init();

    let device_name = "bedroom";
    println!("Looking for AirPlay device named '{}'...", device_name);
    tracing::info!("Starting discovery for device: {}", device_name);

    let mut retry_count = 0;
    let max_retries = 10;
    let mut player = AirPlayPlayer::new();

    loop {
        println!(
            "Attempt {}/{}: Connecting to '{}'...",
            retry_count + 1,
            max_retries,
            device_name
        );

        // Scan and list all devices first for debugging
        println!("Scanning network...");
        let scan_result = player
            .client()
            .scan(std::time::Duration::from_secs(3))
            .await;

        let mut target_device = None;

        match scan_result {
            Ok(devices) => {
                println!("Found {} devices:", devices.len());
                for d in &devices {
                    println!(
                        " - Name: '{}', ID: '{}', IP: {:?}, Port: {}",
                        d.name, d.id, d.addresses, d.port
                    );
                    if d.name.contains(device_name)
                        || d.name.contains("bedroom")
                        || d.id == "DC:9B:9C:EF:90:E9"
                    {
                        target_device = Some(d.clone());
                    }
                }
            }
            Err(e) => println!("Scan failed: {}", e),
        }

        if let Some(device) = target_device {
            println!("Found target device: '{}'. Connecting...", device.name);
            match player.connect(&device).await {
                Ok(_) => {
                    println!("Connected successfully!");
                    break;
                }
                Err(e) => {
                    eprintln!("Connection failed: {:?}", e);
                    retry_count += 1;
                }
            }
        } else {
            println!("Target device '{}' not found in scan.", device_name);
            retry_count += 1;
        }

        if retry_count >= max_retries {
            tracing::error!("Connection failure after {} attempts", max_retries);
            return Ok(());
        }
        println!("Retrying in 2 seconds...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    if let Some(device) = player.device().await {
        println!("Successfully connected to: {} ({})", device.name, device.id);
        println!("  Address: {}:{}", device.address(), device.port);
    }

    // Generate Sine Wave Audio (5 seconds)
    // 44100Hz, Stereo, 16-bit
    let sample_rate = 44100;
    let duration_secs = 5;
    let num_samples = sample_rate * duration_secs;
    let frequency = 440.0; // A4
    let amplitude = 0.5 * (i16::MAX as f32);

    let mut samples = Vec::with_capacity(num_samples * 2);
    for i in 0..num_samples {
        let t = (i as f32) / (sample_rate as f32);
        let sample = (t * frequency * 2.0 * std::f32::consts::PI).sin() * amplitude;
        let sample_i16 = sample as i16;

        // Stereo: Left and Right same
        samples.push(sample_i16);
        samples.push(sample_i16);
    }

    println!("Generated {} samples of sine wave.", samples.len() / 2);

    let format = AudioFormat {
        sample_rate: airplay2::audio::SampleRate::Hz44100,
        channels: airplay2::audio::ChannelConfig::Stereo,
        sample_format: airplay2::audio::SampleFormat::I16,
    };

    let source = SliceSource::from_i16(&samples, format);

    println!("Streaming audio (PCM)...");

    // Use client_mut() to access stream_audio
    player.client_mut().stream_audio(source).await?;

    println!("\nPlayback finished! check your device.");
    println!("Press Enter to stop and exit...");

    // Wait for user input
    let mut stdin = io::BufReader::new(io::stdin());
    let mut line = String::new();
    stdin.read_line(&mut line).await?;

    println!("Stopping playback...");
    player.stop().await?;

    println!("Disconnecting...");
    player.disconnect().await?;

    Ok(())
}
