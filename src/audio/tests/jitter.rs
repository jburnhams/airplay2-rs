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
