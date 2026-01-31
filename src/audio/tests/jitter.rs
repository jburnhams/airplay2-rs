use crate::audio::jitter::*;

#[test]
fn test_in_order_packets() {
    let mut buffer = JitterBuffer::new(2, 10);

    buffer.push(0, "packet0");
    buffer.push(1, "packet1");
    buffer.push(2, "packet2");

    assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
    assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
}

#[test]
fn test_out_of_order_packets() {
    let mut buffer = JitterBuffer::new(2, 10);

    // Packets arrive out of order
    buffer.push(1, "packet1");
    buffer.push(0, "packet0");
    buffer.push(2, "packet2");

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
}

#[test]
fn test_gap_detection() {
    let mut buffer = JitterBuffer::new(1, 10);

    buffer.push(0, "packet0");
    buffer.push(2, "packet2"); // Skip 1

    buffer.pop(); // Get packet0, next expected is 1

    // Next pop should detect gap
    match buffer.pop() {
        NextPacket::Gap {
            expected: 1,
            available: 2,
        } => {}
        other => panic!("Expected Gap, got {other:?}"),
    }
}
