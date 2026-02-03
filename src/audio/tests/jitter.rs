use crate::audio::jitter::*;

#[test]
fn test_in_order_packets() {
    let mut buffer = JitterBuffer::new(2, 10);

    assert!(matches!(buffer.push(0, "packet0"), JitterResult::Buffered));
    assert!(matches!(buffer.push(1, "packet1"), JitterResult::Buffered));
    assert!(matches!(buffer.push(2, "packet2"), JitterResult::Buffered));

    assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
}

#[test]
fn test_out_of_order_packets() {
    let mut buffer = JitterBuffer::new(2, 10);

    // Packets arrive out of order
    assert!(matches!(buffer.push(1, "packet1"), JitterResult::Buffered));
    assert!(matches!(buffer.push(0, "packet0"), JitterResult::Buffered));
    assert!(matches!(buffer.push(2, "packet2"), JitterResult::Buffered));

    // Should still come out in order
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
}

#[test]
fn test_duplicate_detection() {
    let mut buffer = JitterBuffer::new(2, 10);

    buffer.push(0, "packet0");
    let result = buffer.push(0, "duplicate");

    assert!(matches!(result, JitterResult::Duplicate));

    // Stats check
    assert_eq!(buffer.stats().packets_duplicate, 1);
}

#[test]
fn test_gap_detection() {
    let mut buffer = JitterBuffer::new(1, 10);

    buffer.push(0, "packet0");
    buffer.push(2, "packet2"); // Skip 1

    assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));

    // Next pop should detect gap
    match buffer.pop() {
        NextPacket::Gap {
            expected: 1,
            available: 2,
        } => {}
        other => panic!("Expected Gap, got {other:?}"),
    }
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

    assert!(matches!(buffer.push(0, "packet0"), JitterResult::Buffered));
    assert!(matches!(buffer.push(1, "packet1"), JitterResult::Buffered));
    assert!(matches!(buffer.push(2, "packet2"), JitterResult::Buffered));

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

    assert!(matches!(buffer.push(65534, "packet_high"), JitterResult::Buffered));
    assert!(matches!(buffer.push(65535, "packet_max"), JitterResult::Buffered));
    assert!(matches!(buffer.push(0, "packet_zero"), JitterResult::Buffered));
    assert!(matches!(buffer.push(1, "packet_one"), JitterResult::Buffered));

    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_high")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_max")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_zero")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet_one")));
}

#[test]
fn test_skip_to() {
    let mut buffer = JitterBuffer::new(2, 10);

    assert!(matches!(buffer.push(0, "packet0"), JitterResult::Buffered));
    assert!(matches!(buffer.push(1, "packet1"), JitterResult::Buffered));

    buffer.skip_to(10);

    // Old packets should be cleared (or at least ignored/removed)
    assert_eq!(buffer.depth(), 0);

    assert!(matches!(buffer.push(10, "packet10"), JitterResult::Buffered));
    assert!(matches!(buffer.pop(), NextPacket::Wait)); // Waiting for depth

    assert!(matches!(buffer.push(11, "packet11"), JitterResult::Buffered));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet10")));
}

#[test]
fn test_clear() {
    let mut buffer = JitterBuffer::new(2, 10);
    assert!(matches!(buffer.push(0, "packet0"), JitterResult::Buffered));
    assert!(matches!(buffer.push(1, "packet1"), JitterResult::Buffered));

    assert_eq!(buffer.depth(), 2);

    buffer.clear();
    assert_eq!(buffer.depth(), 0);
    assert!(matches!(buffer.pop(), NextPacket::Wait));
}
