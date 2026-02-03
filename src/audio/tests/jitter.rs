use crate::audio::jitter::{BufferState, JitterBuffer, JitterBufferConfig};
use crate::receiver::rtp_receiver::AudioPacket;
use std::time::Instant;

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
fn test_in_order_packets() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 3,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    buffer.insert(make_packet(2, 704));
    buffer.insert(make_packet(3, 1056));

    assert!(buffer.is_ready());
    assert_eq!(buffer.depth(), 3);

    let p1 = buffer.pop().unwrap();
    assert_eq!(p1.sequence, 1);

    let p2 = buffer.pop().unwrap();
    assert_eq!(p2.sequence, 2);
}

#[test]
fn test_out_of_order_packets() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 3,
        ..Default::default()
    });

    // Insert out of order
    buffer.insert(make_packet(3, 1056));
    buffer.insert(make_packet(1, 352));
    buffer.insert(make_packet(2, 704));

    assert!(buffer.is_ready());

    // Should pop in order
    assert_eq!(buffer.pop().unwrap().sequence, 1);
    assert_eq!(buffer.pop().unwrap().sequence, 2);
    assert_eq!(buffer.pop().unwrap().sequence, 3);
}

#[test]
fn test_buffering_state() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 3,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    assert!(!buffer.is_ready());

    buffer.insert(make_packet(2, 704));
    assert!(!buffer.is_ready());

    buffer.insert(make_packet(3, 1056));
    assert!(buffer.is_ready());
}

#[test]
fn test_late_packet_dropped() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(10, 3520));
    buffer.insert(make_packet(11, 3872));

    // Pop first packet
    buffer.pop();

    // Now insert a late packet (seq 5, before current playback)
    buffer.insert(make_packet(5, 1760));

    assert_eq!(buffer.stats().packets_dropped_late, 1);
}

#[test]
fn test_very_late_packet_dropped() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(10, 3520));
    buffer.insert(make_packet(11, 3872));

    // Start playback (next = 10)
    let p = buffer.pop();
    assert_eq!(p.unwrap().sequence, 10);
    // next = 11

    // Insert a very late packet (seq = 11 - 2000 = 63547 approx with u16 wrapping)
    // Actually, let's just use 11 - 2000.
    // u16: 11 - 2000 = 63547.
    // 63547 - 11 = 63536. 63536 as i16 is negative (-1999).
    let very_late_seq = 11u16.wrapping_sub(2000);
    buffer.insert(make_packet(very_late_seq, 0));

    assert_eq!(buffer.stats().packets_dropped_late, 1);
    assert_eq!(buffer.depth(), 1); // Should only have seq 11 left
}

#[test]
fn test_flush() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    buffer.insert(make_packet(2, 704));
    buffer.insert(make_packet(3, 1056));

    buffer.flush();

    assert_eq!(buffer.depth(), 0);
    assert_eq!(buffer.state(), BufferState::Buffering);
}

#[test]
fn test_underrun() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    buffer.insert(make_packet(2, 704));

    buffer.pop();
    buffer.pop();

    // Buffer now empty
    let result = buffer.pop();
    assert!(result.is_none());
    assert_eq!(buffer.state(), BufferState::Underrun);
}

#[test]
fn test_gap_skip_updates_state() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(1, 352));
    // Gap: skip 2
    buffer.insert(make_packet(3, 1056));

    // Play 1. next=2.
    let p1 = buffer.pop();
    assert_eq!(p1.unwrap().sequence, 1);
    assert_eq!(buffer.state(), BufferState::Playing);
    assert_eq!(buffer.depth(), 1);

    // Pop next. Expecting 2, have 3. Gap=1 (<10). Should skip to 3.
    // This removes 3. Buffer becomes empty. State should update to Underrun.
    let p3 = buffer.pop();
    assert_eq!(p3.unwrap().sequence, 3);

    // Verify state updated
    assert_eq!(buffer.depth(), 0);
    assert_eq!(buffer.stats().current_depth, 0);
    assert_eq!(buffer.state(), BufferState::Underrun);
}

#[test]
fn test_wraparound_sequence() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig {
        min_depth: 2,
        ..Default::default()
    });

    buffer.insert(make_packet(65534, 0));
    buffer.insert(make_packet(65535, 352));
    buffer.insert(make_packet(0, 704));

    assert_eq!(buffer.pop().unwrap().sequence, 65534);
    assert_eq!(buffer.pop().unwrap().sequence, 65535);
    assert_eq!(buffer.pop().unwrap().sequence, 0);
}

#[test]
fn test_late_packet() {
    let mut buffer = JitterBuffer::new(1, 10);

    // Advance next_seq to 1000
    buffer.skip_to(1000);

    // Packet 800 (200 late) should be rejected
    // 1000 - 200 = 800.
    // Threshold is 100.
    let result = buffer.push(800, "packet800_late");
    assert!(matches!(result, JitterResult::TooLate));

    assert_eq!(buffer.stats().packets_late, 1);
}

#[test]
fn test_slightly_late_packet_kept() {
    let mut buffer = JitterBuffer::new(1, 10);
    buffer.skip_to(100);

    // Packet 99 (1 late) should be kept (buffered), even if not useful
    let result = buffer.push(99, "packet99_late");
    assert!(matches!(result, JitterResult::Buffered));
}

#[test]
fn test_buffer_overflow() {
    let mut buffer = JitterBuffer::new(10, 3); // Max size 3

    buffer.push(0, "packet0");
    buffer.push(1, "packet1");
    buffer.push(2, "packet2");

    // Push 4th packet, should cause overflow of oldest (0)
    match buffer.push(3, "packet3") {
        JitterResult::Overflow(p) => assert_eq!(p, "packet0"),
        other => panic!("Expected Overflow, got {other:?}"),
    }

    assert_eq!(buffer.stats().packets_overflow, 1);
    assert_eq!(buffer.depth(), 3);
}

#[test]
fn test_wrapping_sequence() {
    let mut buffer = JitterBuffer::new(1, 10); // Target depth 1 to allow draining

    // Start near end of u16 range
    buffer.skip_to(65534);

    buffer.push(65534, "packet_high");
    buffer.push(65535, "packet_max");
    buffer.push(0, "packet_zero");
    buffer.push(1, "packet_one");

    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_high")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_max")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_zero")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_one")));
}

#[test]
fn test_skip_to() {
    let mut buffer = JitterBuffer::new(2, 10);

    buffer.push(0, "packet0");
    buffer.push(1, "packet1");

    buffer.skip_to(10);

    // Old packets should be cleared (or at least ignored/removed)
    assert_eq!(buffer.depth(), 0);

    buffer.push(10, "packet10");
    assert!(matches!(buffer.pop(), NextPacket::Wait)); // Waiting for depth

    buffer.push(11, "packet11");
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet10")));
}

#[test]
fn test_clear() {
    let mut buffer = JitterBuffer::new(2, 10);
    buffer.push(0, "packet0");
    buffer.push(1, "packet1");

    assert_eq!(buffer.depth(), 2);

    buffer.clear();
    assert_eq!(buffer.depth(), 0);
    assert!(matches!(buffer.pop(), NextPacket::Wait));
}
