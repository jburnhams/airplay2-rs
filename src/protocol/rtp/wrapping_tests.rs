use super::packet_buffer::PacketLossDetector;

#[test]
fn test_wrap_basic() {
    let mut detector = PacketLossDetector::new();

    // Initial sequence
    detector.process(65534);
    detector.process(65535);

    // Wrap around to 0
    let missing = detector.process(0);
    assert!(missing.is_empty(), "Should handle wrap from 65535 to 0");

    let missing = detector.process(1);
    assert!(missing.is_empty());
}

#[test]
fn test_wrap_with_loss_at_boundary() {
    let mut detector = PacketLossDetector::new();

    detector.process(65535);

    // Skip 0, receive 1
    // Expected: 0 should be reported missing
    let missing = detector.process(1);
    assert_eq!(missing, vec![0]);
}

#[test]
fn test_wrap_with_loss_before_boundary() {
    let mut detector = PacketLossDetector::new();

    detector.process(65533);

    // Skip 65534, 65535, 0, receive 1
    // Expected: 65534, 65535, 0
    let missing = detector.process(1);
    assert_eq!(missing, vec![65534, 65535, 0]);
}

#[test]
fn test_wrap_reorder_old_packet() {
    let mut detector = PacketLossDetector::new();

    detector.process(65535);
    detector.process(0); // Wrapped, now expecting 1

    // Receive old packet from before wrap
    let missing = detector.process(65534);
    assert!(
        missing.is_empty(),
        "Should ignore old packet 65534 when at 0"
    );
}

#[test]
fn test_wrap_reorder_future_packet() {
    let mut detector = PacketLossDetector::new();

    detector.process(65535);

    // Receive 1 (gap of 0)
    let missing = detector.process(1);
    assert_eq!(missing, vec![0]);

    // Receive 0 (the missing one) - technically "old" now relative to expected 2
    // But since it's just 2 back, it should be ignored by the detector logic as "old"
    // (detector only tracks gaps forward).
    // The jitter buffer would handle the actual storage/retrieval.
    let missing = detector.process(0);
    assert!(missing.is_empty());
}

#[test]
fn test_large_gap_across_wrap() {
    let mut detector = PacketLossDetector::new();

    detector.process(65500);

    // Jump to 50 (gap of ~86 packets)
    // 65501..65535 (35 packets) + 0..49 (50 packets) = 85 packets
    let missing = detector.process(50);

    assert_eq!(missing.len(), 85);
    assert_eq!(missing[0], 65501);
    assert_eq!(missing.last(), Some(&49));
}

#[test]
fn test_multiple_wraps_simulation() {
    let mut detector = PacketLossDetector::new();
    let mut current = 0u16;

    // Simulate 3 full wraps
    for _ in 0..(65536 * 3) {
        let missing = detector.process(current);
        assert!(missing.is_empty());
        current = current.wrapping_add(1);
    }
}

#[test]
fn test_max_gap_limit() {
    // Implementation limits reported gap to < 100 packets
    let mut detector = PacketLossDetector::new();

    detector.process(0);

    // Jump to 100 (gap of 99 packets: 1..99)
    let missing = detector.process(100);
    assert_eq!(missing.len(), 99);

    // Reset
    let mut detector = PacketLossDetector::new();
    detector.process(0);

    // Jump to 101 (gap of 100 packets: 1..100)
    // Should be ignored/resynced without reporting loss
    let missing = detector.process(101);
    assert!(missing.is_empty());
}
