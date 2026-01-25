use airplay2::audio::AudioFormat;
use airplay2::protocol::daap::TrackMetadata;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig, AirPlayDevice, DeviceCapabilities};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

struct TestSource {
    phase: f32,
    freq: f32,
    format: AudioFormat,
}

impl TestSource {
    fn new() -> Self {
        Self {
            phase: 0.0,
            freq: 440.0,
            format: AudioFormat::CD_QUALITY,
        }
    }
}

impl AudioSource for TestSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;
        for chunk in buffer.chunks_exact_mut(4) {
            let sample = (self.phase * 2.0 * PI).sin();
            let value = (sample * i16::MAX as f32 * 0.5) as i16;
            let bytes = value.to_ne_bytes();
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
            chunk[2] = bytes[0];
            chunk[3] = bytes[1];
            self.phase = (self.phase + self.freq / sample_rate) % 1.0;
        }
        Ok(buffer.len())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("airplay2=info")
        .init();

    let device = AirPlayDevice {
        id: "ac:07:75:12:4a:1f".to_string(),
        name: "airplay2-rs-test".to_string(),
        model: Some("Receiver".to_string()),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 0, 101))],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: HashMap::new(),
    };

    let config = AirPlayConfig::default();
    let client = AirPlayClient::new(config);

    client.connect(&device).await?;

    println!("Starting stream...");
    let source = TestSource::new();
    let mut client_clone = client.clone();
    tokio::spawn(async move {
        let _ = client_clone.stream_audio(source).await;
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("Setting Metadata: Title='Antigravity', Artist='DeepMind', Album='Advanced Coding'");
    let metadata = TrackMetadata::builder()
        .title("Antigravity")
        .artist("DeepMind")
        .album("Advanced Coding")
        .duration_ms(300000)
        .build();
    client.set_metadata(metadata).await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Pausing...");
    client.pause().await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Resuming...");
    client.play().await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Seeking to 1 minute...");
    client.seek(Duration::from_secs(60)).await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Stopping...");
    client.stop().await?;

    println!("Done.");
    Ok(())
}
