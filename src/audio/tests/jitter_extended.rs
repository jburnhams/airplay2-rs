use std::time::Instant;

use crate::audio::jitter::{BufferState, JitterBuffer, JitterBufferConfig};
use crate::receiver::rtp_receiver::AudioPacket;

fn make_packet(seq: u16, timestamp: u32) -> AudioPacket {
    AudioPacket {
        sequence: seq,
        timestamp,
        ssrc: 0x1234_5678,
        audio_data: vec![0u8; 1408],
        received_at: Instant::now(),
    }
}

#[test]
fn test_buffer_overflow() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 5,
        max_depth: 10,
        ..Default::default()
    });

    // Fill buffer to max
    for i in 0..10 {
        buffer.insert(make_packet(i, u32::from(i) * 352));
    }
    assert_eq!(buffer.depth(), 10);
    assert_eq!(buffer.stats().packets_dropped_overflow, 0);

    // Insert one more
    buffer.insert(make_packet(10, 3520));

    // Should still be 10 (one dropped, or oldest removed)
    assert_eq!(buffer.depth(), 10);
    assert_eq!(buffer.stats().packets_dropped_overflow, 1);

    // Verify oldest (seq 0) was dropped, so next pop should be 1
    // The buffer logic does NOT automatically advance next_play_seq when dropping.
    // So pop() will look for 0. It's gone.
    // handle_missing_packet(0) will be called.
    // It checks if next available (1) is close (<10 packets away). 1 - 0 = 1.
    // So it should skip to 1.

    let p = buffer.pop();
    assert!(p.is_some());
    assert_eq!(p.unwrap().sequence, 1);
    assert_eq!(buffer.stats().packets_concealed, 1);
}

#[test]
fn test_duplicate_packets() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    buffer.insert(make_packet(1, 352)); // Duplicate

    // Map prevents duplicates, but let's see if stats track it?
    // Implementation: self.packets.insert(seq, packet);
    // It just overwrites. No stat for duplicate in current impl.
    // So depth should be 1.
    assert_eq!(buffer.depth(), 1);
}

#[test]
fn test_large_gap_concealment() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    // Gap > 10
    buffer.insert(make_packet(20, 7040));

    // Start playing
    buffer.pop(); // Returns seq 1

    // Next is 2. But we only have 20.
    // Gap = 18.
    // handle_missing_packet(2) -> gap >= 10.
    // Should return None and advance next_play_seq to 3.

    let p = buffer.pop();
    assert!(p.is_none());
    // next_play_seq is now 3.

    // Verify stats
    assert_eq!(buffer.stats().packets_concealed, 1);
}

#[test]
fn test_buffer_health() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 5,
        target_depth: 10,
        ..Default::default()
    });

    for i in 0..5 {
        buffer.insert(make_packet(i, u32::from(i) * 352));
    }

    let health = buffer.health();
    assert_eq!(health.depth, 5);
    assert!(health.is_healthy);
    assert!((health.fill_ratio - 0.5).abs() < 0.001); // 5/10 = 0.5
}

#[test]
fn test_buffer_unhealthy_underrun() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 5,
        ..Default::default()
    });

    // Only 1 packet
    buffer.insert(make_packet(1, 352));

    let health = buffer.health();
    // Depth 1 < min_depth/2 (2.5) => unhealthy?
    // is_healthy: depth >= self.config.min_depth / 2 && depth <= self.config.max_depth
    // 1 < 2, so false.
    assert!(!health.is_healthy);
}
