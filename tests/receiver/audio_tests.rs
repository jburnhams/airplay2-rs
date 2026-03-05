//! Audio Quality Tests

use airplay2::receiver::ap2::jitter_buffer::{JitterBuffer, JitterBufferConfig, BufferState};
use airplay2::receiver::ap2::rtp_receiver::AudioFrame;
use std::time::Instant;

/// Test jitter buffer maintains audio continuity
#[test]
fn test_jitter_buffer_continuity() {
    let config = JitterBufferConfig {
        target_depth_ms: 100,
        sample_rate: 44100,
        channels: 2,
        ..Default::default()
    };

    let mut buffer = JitterBuffer::new(config);

    // Add frames in order
    for i in 0..50u16 {
        let frame = AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![i as i16; 704],  // 352 stereo samples
            receive_time: Instant::now(),
        };
        buffer.push(frame);
    }

    assert_eq!(buffer.state(), BufferState::Playing);

    // Pull samples and verify continuity
    let samples = buffer.pull(352);
    assert_eq!(samples.len(), 704);  // Stereo
}

/// Test jitter buffer handles packet loss
#[test]
fn test_jitter_buffer_loss_concealment() {
    let config = JitterBufferConfig {
        target_depth_ms: 100,
        sample_rate: 44100,
        channels: 2,
        ..Default::default()
    };

    let mut buffer = JitterBuffer::new(config);

    // Add frames with gap
    for i in 0..20u16 {
        buffer.push(AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![100i16; 704],
            receive_time: Instant::now(),
        });
    }

    // Skip frame 20, add 21-30
    for i in 21..30u16 {
        buffer.push(AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![100i16; 704],
            receive_time: Instant::now(),
        });
    }

    assert!(buffer.stats().frames_lost > 0, "Should detect lost frame");
}

/// Test audio decryption with known vectors
#[test]
fn test_audio_decryption() {
    // This would use known test vectors
    // For now, just verify the decryptor can be created

    use airplay2::receiver::ap2::rtp_decryptor::Ap2RtpDecryptor;

    let key = [0x42u8; 32];
    let _decryptor = Ap2RtpDecryptor::new(key);

    // Would test with known encrypted payload
}
