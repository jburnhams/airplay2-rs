use bytes::Bytes;

use crate::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer, PacketLossDetector};

#[test]
fn test_packet_buffer_out_of_order() {
    let mut buffer = PacketBuffer::new(10);
    // Push 10, then 8, then 9
    buffer.push(BufferedPacket {
        sequence: 10,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 8,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 9,
        timestamp: 0,
        data: Bytes::new(),
    });

    // Request 8, 9, 10 - count 3
    let mut iter = buffer.get_range(8, 3);

    // get_range should find them regardless of insertion order
    let p1 = iter.next().expect("Expected packet 8");
    assert_eq!(p1.sequence, 8);

    let p2 = iter.next().expect("Expected packet 9");
    assert_eq!(p2.sequence, 9);

    let p3 = iter.next().expect("Expected packet 10");
    assert_eq!(p3.sequence, 10);
}

#[test]
fn test_packet_buffer_wrapping_out_of_order() {
    let mut buffer = PacketBuffer::new(10);
    // Push in order: 65535, 1, 0
    buffer.push(BufferedPacket {
        sequence: 65535,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 1,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 0,
        timestamp: 0,
        data: Bytes::new(),
    });

    // Request range crossing wrap: 65535, 0, 1
    let mut iter = buffer.get_range(65535, 3);

    let p1 = iter.next().expect("Expected packet 65535");
    assert_eq!(p1.sequence, 65535);

    let p2 = iter.next().expect("Expected packet 0");
    assert_eq!(p2.sequence, 0);

    let p3 = iter.next().expect("Expected packet 1");
    assert_eq!(p3.sequence, 1);
}

#[test]
fn test_packet_buffer_gap_handling() {
    let mut buffer = PacketBuffer::new(10);
    // Push 10, skip 11, push 12
    buffer.push(BufferedPacket {
        sequence: 10,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 12,
        timestamp: 0,
        data: Bytes::new(),
    });

    // Request 10, 11, 12
    let mut iter = buffer.get_range(10, 3);

    let p1 = iter.next().expect("Expected packet 10");
    assert_eq!(p1.sequence, 10);

    // Should skip 11 (missing) and return 12
    let p2 = iter.next().expect("Expected packet 12");
    assert_eq!(p2.sequence, 12);

    assert!(iter.next().is_none());
}

#[test]
fn test_packet_loss_detector_large_jump() {
    let mut detector = PacketLossDetector::new();
    detector.process(100);

    // Jump 200 packets (larger than threshold 100)
    let missing = detector.process(301);

    // Should NOT report missing packets because jump > 100
    assert!(missing.is_empty());

    // Should update expected to 302
    let missing = detector.process(303);
    assert_eq!(missing, vec![302]);
}

#[test]
fn test_packet_loss_detector_exact_threshold() {
    let mut detector = PacketLossDetector::new();
    detector.process(100);
    // Expected 101

    // Jump exactly 99 packets (sequence 200) -> diff = 99
    // Logic: diff > 0 && diff < 100
    // So 99 is accepted as loss
    let missing = detector.process(200);
    assert_eq!(missing.len(), 99);
    assert_eq!(missing[0], 101);
    assert_eq!(missing[98], 199);
}

#[test]
fn test_packet_loss_detector_boundary_threshold() {
    let mut detector = PacketLossDetector::new();
    detector.process(100);
    // Expected 101

    // Jump 100 packets (sequence 201) -> diff = 100
    // Logic: diff < 100 -> false
    // So 100 is treated as a reset/jump
    let missing = detector.process(201);
    assert!(missing.is_empty());
}
