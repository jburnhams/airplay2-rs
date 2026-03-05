//! Test Utilities for `AirPlay` 2 Receiver

use crate::receiver::ap2::{AirPlay2Receiver, Ap2Config};

/// Create a test receiver with random port
///
/// # Panics
/// Panics if a free port cannot be picked.
#[must_use]
pub fn create_test_receiver() -> (AirPlay2Receiver, u16) {
    // Bind to a system-assigned port and retrieve it, then release it
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind to free port");
    let port = listener
        .local_addr()
        .expect("Failed to get local addr")
        .port();
    drop(listener);

    let config = Ap2Config::new("Test Receiver").with_port(port);

    let receiver = AirPlay2Receiver::new(config);
    (receiver, port)
}

/// Generate test audio data (sine wave)
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Precision loss and sign loss are acceptable for generating basic sine wave samples \
              in tests."
)]
pub fn generate_test_audio(
    frequency: f32,
    sample_rate: u32,
    duration_ms: u32,
    channels: u8,
) -> Vec<i16> {
    let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;
    let mut samples = Vec::with_capacity(num_samples * channels as usize);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let value = (2.0 * std::f32::consts::PI * frequency * t).sin();
        let sample = (value * 16000.0) as i16;

        for _ in 0..channels {
            samples.push(sample);
        }
    }

    samples
}

/// Compare audio samples with tolerance
#[must_use]
pub fn samples_match(a: &[i16], b: &[i16], tolerance: i16) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for (sa, sb) in a.iter().zip(b.iter()) {
        if (sa - sb).abs() > tolerance {
            return false;
        }
    }

    true
}

/// Wait for condition with timeout
pub async fn wait_for<F>(condition: F, timeout_ms: u64, check_interval_ms: u64) -> bool
where
    F: Fn() -> bool,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let interval = std::time::Duration::from_millis(check_interval_ms);

    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(interval).await;
    }

    false
}
