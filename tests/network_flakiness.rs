use airplay2::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer, PacketLossDetector};

#[test]
fn test_recovery_from_packet_loss() {
    let mut buffer = PacketBuffer::new(200);
    let mut detector = PacketLossDetector::new();

    // Simulate sending 100 packets
    let mut all_packets = Vec::new();
    for i in 0u16..100 {
        all_packets.push(BufferedPacket {
            sequence: i,
            timestamp: u32::from(i) * 352,
            data: vec![0xAA; 10].into(),
        });
    }

    // Simulate network: drop packets 50-54 (5 packets)
    // Receive 0..49
    for packet in all_packets.iter().take(50) {
        detector.process(packet.sequence);
        buffer.push(packet.clone());
    }

    // Receive 55..99
    // When 55 arrives, detector should notice gap 50..54
    let mut missing_reported = Vec::new();

    for packet in &all_packets[55..] {
        let missing = detector.process(packet.sequence);
        missing_reported.extend(missing);
        buffer.push(packet.clone());
    }

    // Verify missing packets were reported
    assert_eq!(missing_reported, vec![50, 51, 52, 53, 54]);

    // Check buffer state: 50..54 are missing
    assert!(buffer.get(49).is_some());
    assert!(buffer.get(50).is_none());
    assert!(buffer.get(54).is_none());
    assert!(buffer.get(55).is_some());

    // Simulate Retransmission: Fill the gaps
    for seq in missing_reported {
        // "Fetch" from source
        let packet = &all_packets[seq as usize];
        buffer.push(packet.clone());
    }

    // Verify buffer is now complete
    for i in 0u16..100 {
        assert!(
            buffer.get(i).is_some(),
            "Packet {} should be present after retransmission",
            i
        );
    }
}

#[test]
fn test_reordered_packets_handling() {
    let mut buffer = PacketBuffer::new(100);
    let mut detector = PacketLossDetector::new();

    let p1 = BufferedPacket {
        sequence: 1,
        timestamp: 352,
        data: vec![].into(),
    };
    let p2 = BufferedPacket {
        sequence: 2,
        timestamp: 704,
        data: vec![].into(),
    };
    let p3 = BufferedPacket {
        sequence: 3,
        timestamp: 1056,
        data: vec![].into(),
    };

    // Receive 1, then 3 (gap 2), then 2 (reordered)

    // 1. Recv 1
    detector.process(p1.sequence);
    buffer.push(p1);

    // 2. Recv 3. Detector reports 2 missing.
    let missing = detector.process(p3.sequence);
    assert_eq!(missing, vec![2]);
    buffer.push(p3);

    // 3. Recv 2. Detector sees it as old (diff wrapped/negative/huge).
    // Buffer just accepts it.
    let missing_again = detector.process(p2.sequence);
    assert!(missing_again.is_empty());
    buffer.push(p2);

    // Buffer should have all 3
    assert!(buffer.get(1).is_some());
    assert!(buffer.get(2).is_some());
    assert!(buffer.get(3).is_some());
}
