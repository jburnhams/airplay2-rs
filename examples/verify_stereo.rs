use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;
use airplay2::{AirPlayClient, AirPlayConfig, AirPlayDevice, DeviceCapabilities, scan};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

/// Stereo test source: 440Hz Left, 880Hz Right
struct StereoSource {
    phase_l: f32,
    phase_r: f32,
    freq_l: f32,
    freq_r: f32,
    format: AudioFormat,
    samples_generated: u64,
    max_samples: u64,
}

impl StereoSource {
    fn new(duration_secs: u32) -> Self {
        let format = AudioFormat::CD_QUALITY; // 16-bit 44.1kHz stereo
        let max_samples = u64::from(duration_secs) * u64::from(format.sample_rate.as_u32());
        Self {
            phase_l: 0.0,
            phase_r: 0.0,
            freq_l: 440.0,
            freq_r: 880.0,
            format,
            samples_generated: 0,
            max_samples,
        }
    }
}

impl AudioSource for StereoSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;

        if self.samples_generated >= self.max_samples {
            return Ok(0); // EOF
        }

        let mut bytes_written = 0;
        // Process 4 bytes at a time (2 bytes Left + 2 bytes Right)
        for chunk in buffer.chunks_exact_mut(4) {
            if self.samples_generated >= self.max_samples {
                break;
            }

            // Left Channel (440Hz)
            let sample_l = (self.phase_l * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value_l = (sample_l * i16::MAX as f32 * 0.5) as i16;
            let bytes_l = value_l.to_ne_bytes();

            // Right Channel (880Hz)
            let sample_r = (self.phase_r * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value_r = (sample_r * i16::MAX as f32 * 0.5) as i16;
            let bytes_r = value_r.to_ne_bytes();

            // Interleaved: L, R
            chunk[0] = bytes_l[0];
            chunk[1] = bytes_l[1];
            chunk[2] = bytes_r[0];
            chunk[3] = bytes_r[1];

            // Update phases
            self.phase_l += self.freq_l / sample_rate;
            if self.phase_l > 1.0 {
                self.phase_l -= 1.0;
            }

            self.phase_r += self.freq_r / sample_rate;
            if self.phase_r > 1.0 {
                self.phase_r -= 1.0;
            }

            self.samples_generated += 1;
            bytes_written += 4;
        }

        Ok(bytes_written)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("airplay2=info")
        .init();

    println!("Scanning for devices...");
    let devices = scan(Duration::from_secs(2)).await?;

    let device = if let Some(d) = devices.iter().find(|d| d.name == "airplay2-rs-test") {
        println!("Found discovered local receiver: {}", d.name);
        d.clone()
    } else {
        println!("Local receiver not found via mDNS. Using fallback...");
        // Manual device construction for test environment
        let ip = Ipv4Addr::new(192, 168, 0, 101);
        AirPlayDevice {
            id: "ac:07:75:12:4a:1f".to_string(),
            name: "airplay2-rs-test".to_string(),
            model: Some("Receiver".to_string()),
            addresses: vec![IpAddr::V4(ip)],
            port: 7000,
            capabilities: DeviceCapabilities::default(),
            raop_port: None,
            raop_capabilities: None,
            txt_records: HashMap::new(),
        }
    };

    let config = AirPlayConfig::default();
    let client = AirPlayClient::new(config);

    println!("Connecting to {} ({:?})...", device.name, device.addresses);
    client.connect(&device).await?;

    println!("Streaming Full Verification (Stereo + Volume)...");
    let source = StereoSource::new(10); // 10 seconds
    
    // Start streaming in a separate task so we can control volume
    let mut client_clone = client.clone();
    let stream_handle = tokio::spawn(async move {
        client_clone.stream_audio(source).await
    });

    // Test Volume Control
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Setting volume to 25%...");
    client.set_volume(0.25).await?;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Setting volume to 100%...");
    client.set_volume(1.0).await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Muting...");
    client.mute().await?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("Unmuting...");
    client.unmute().await?;

    // Wait for stream to finish
    stream_handle.await??;

    println!("Done.");
    Ok(())
}
