use crate::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer, PacketLossDetector};
use bytes::Bytes;

#[test]
fn test_packet_buffer_new() {
    let buffer = PacketBuffer::new(10);
    assert!(buffer.is_empty());
    assert_eq!(buffer.len(), 0);
    assert_eq!(buffer.sequence_range(), None);
}

#[test]
fn test_packet_buffer_push_and_get() {
    let mut buffer = PacketBuffer::new(5);

    let packet = BufferedPacket {
        sequence: 100,
        timestamp: 12345,
        data: vec![1, 2, 3].into(),
    };

    buffer.push(packet);

    assert!(!buffer.is_empty());
    assert_eq!(buffer.len(), 1);

    let retrieved = buffer.get(100).unwrap();
    assert_eq!(retrieved.sequence, 100);
    assert_eq!(retrieved.data, Bytes::from(vec![1, 2, 3]));

    assert!(buffer.get(99).is_none());
}

#[test]
fn test_packet_buffer_overflow() {
    let mut buffer = PacketBuffer::new(3);

    for i in 0..5 {
        buffer.push(BufferedPacket {
            sequence: i,
            timestamp: u32::from(i),
            data: Bytes::new(),
        });
    }

    assert_eq!(buffer.len(), 3);
    // Should have 2, 3, 4
    assert!(buffer.get(0).is_none());
    assert!(buffer.get(1).is_none());
    assert!(buffer.get(2).is_some());
    assert!(buffer.get(4).is_some());
}

#[test]
fn test_packet_buffer_get_range() {
    let mut buffer = PacketBuffer::new(10);

    for i in 100..105 {
        buffer.push(BufferedPacket {
            sequence: i,
            timestamp: u32::from(i),
            data: Bytes::new(),
        });
    }

    let range = buffer.get_range(101, 3); // 101, 102, 103
    assert_eq!(range.len(), 3);
    assert_eq!(range[0].sequence, 101);
    assert_eq!(range[2].sequence, 103);

    // Partial range
    let range = buffer.get_range(103, 5); // 103, 104
    assert_eq!(range.len(), 2);
    assert_eq!(range[0].sequence, 103);
    assert_eq!(range[1].sequence, 104);
}

#[test]
fn test_packet_buffer_get_range_wrapping() {
    let mut buffer = PacketBuffer::new(10);

    // Push packets around the wrapping boundary
    // 65534, 65535, 0, 1
    let seqs = [65534, 65535, 0, 1];
    for &seq in &seqs {
        buffer.push(BufferedPacket {
            sequence: seq,
            timestamp: u32::from(seq),
            data: vec![],
        });
    }

    // Request range crossing the boundary: 65535, 0
    // start=65535, count=2 -> start+count = 1 (wrapped)
    // Range 65535..1 is empty in Rust!
    let range = buffer.get_range(65535, 2);

    assert_eq!(range.len(), 2, "Should return 2 packets for wrapping range");
    assert_eq!(range[0].sequence, 65535);
    assert_eq!(range[1].sequence, 0);
}

#[test]
fn test_packet_buffer_clear() {
    let mut buffer = PacketBuffer::new(5);
    buffer.push(BufferedPacket {
        sequence: 1,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.clear();
    assert!(buffer.is_empty());
}

#[test]
fn test_sequence_range() {
    let mut buffer = PacketBuffer::new(5);
    assert_eq!(buffer.sequence_range(), None);

    buffer.push(BufferedPacket {
        sequence: 10,
        timestamp: 0,
        data: Bytes::new(),
    });
    assert_eq!(buffer.sequence_range(), Some((10, 10)));

    buffer.push(BufferedPacket {
        sequence: 11,
        timestamp: 0,
        data: Bytes::new(),
    });
    assert_eq!(buffer.sequence_range(), Some((10, 11)));
}

#[test]
fn test_loss_detector_sequential() {
    let mut detector = PacketLossDetector::new();

    let missing = detector.process(100);
    assert!(missing.is_empty());

    let missing = detector.process(101);
    assert!(missing.is_empty());

    let missing = detector.process(102);
    assert!(missing.is_empty());
}

#[test]
fn test_loss_detector_gap() {
    let mut detector = PacketLossDetector::new();

    detector.process(100);

    // Skip 101, receive 102
    let missing = detector.process(102);
    assert_eq!(missing, vec![101]);

    // Skip 103, 104, receive 105
    let missing = detector.process(105);
    assert_eq!(missing, vec![103, 104]);
}

#[test]
fn test_loss_detector_wrapping() {
    let mut detector = PacketLossDetector::new();

    detector.process(65534);
    detector.process(65535);

    // Wrap to 0
    let missing = detector.process(0);
    assert!(missing.is_empty());

    // Gap across wrap: 65535 -> (miss 0) -> 1
    // Reset
    let mut detector = PacketLossDetector::new();
    detector.process(65535);
    let missing = detector.process(1); // Expect 0
    assert_eq!(missing, vec![0]);
}

#[test]
fn test_loss_detector_reorder() {
    let mut detector = PacketLossDetector::new();

    detector.process(100);
    // expected is 101

    // Receive old packet, should be ignored/empty
    let missing = detector.process(99);
    assert!(missing.is_empty());

    // Receive next expected packet (101)
    // If bug exists, expected_seq was reset to 100 by the reordered packet,
    // so it would report 100 as missing here.
    let missing = detector.process(101);
    assert!(missing.is_empty());
}
