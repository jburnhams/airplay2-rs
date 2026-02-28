use bytes::Bytes;

use crate::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer, PacketLossDetector};
use crate::protocol::rtp::raop::{RaopAudioPacket, RaopPayloadType, RetransmitRequest, SyncPacket};
use crate::protocol::rtp::timing::NtpTimestamp;

#[test]
fn test_sync_packet_encode_decode() {
    let ntp = NtpTimestamp::now();
    let packet = SyncPacket::new(1000, ntp, 1352, true);

    let encoded = packet.encode();
    assert_eq!(encoded.len(), SyncPacket::SIZE);

    let decoded = SyncPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.rtp_timestamp, 1000);
    assert_eq!(decoded.next_timestamp, 1352);
    assert!(decoded.extension);
}

#[test]
fn test_audio_packet_encode_decode() {
    let payload = vec![0x01, 0x02, 0x03, 0x04];
    let packet = RaopAudioPacket::new(100, 44100, 0x1234_5678, payload.clone()).with_marker();

    let encoded = packet.encode();
    let decoded = RaopAudioPacket::decode(&encoded).unwrap();

    assert_eq!(decoded.sequence, 100);
    assert_eq!(decoded.timestamp, 44100);
    assert_eq!(decoded.ssrc, 0x1234_5678);
    assert!(decoded.marker);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn test_retransmit_request_decode() {
    let data = [0x00, 0x0A, 0x00, 0x05]; // seq=10, count=5
    let request = RetransmitRequest::decode(&data).unwrap();

    assert_eq!(request.seq_start, 10);
    assert_eq!(request.count, 5);
}

#[test]
fn test_payload_type_parsing() {
    assert_eq!(
        RaopPayloadType::from_byte(0x60),
        Some(RaopPayloadType::AudioRealtime)
    );
    assert_eq!(
        RaopPayloadType::from_byte(0xE0),
        Some(RaopPayloadType::AudioRealtime)
    ); // With marker
    assert!(RaopPayloadType::AudioRealtime.is_audio());
    assert!(!RaopPayloadType::Sync.is_audio());
}

#[test]
fn test_buffer_push_get() {
    let mut buffer = PacketBuffer::new(10);

    buffer.push(BufferedPacket {
        sequence: 100,
        timestamp: 0,
        data: vec![1, 2, 3].into(),
    });

    let packet = buffer.get(100).unwrap();
    assert_eq!(packet.sequence, 100);
}

#[test]
fn test_buffer_overflow() {
    let mut buffer = PacketBuffer::new(2);

    buffer.push(BufferedPacket {
        sequence: 1,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 2,
        timestamp: 0,
        data: Bytes::new(),
    });
    buffer.push(BufferedPacket {
        sequence: 3,
        timestamp: 0,
        data: Bytes::new(),
    });

    assert!(buffer.get(1).is_none()); // Evicted
    assert!(buffer.get(2).is_some());
    assert!(buffer.get(3).is_some());
}

#[test]
fn test_buffer_range() {
    let mut buffer = PacketBuffer::new(10);

    #[allow(clippy::cast_possible_truncation)]
    for i in 0..5 {
        buffer.push(BufferedPacket {
            sequence: i,
            timestamp: u32::from(i) * 352,
            data: vec![i as u8].into(),
        });
    }

    let range: Vec<_> = buffer.get_range(1, 3).collect();
    assert_eq!(range.len(), 3);
    assert_eq!(range[0].sequence, 1);
    assert_eq!(range[2].sequence, 3);
}

#[test]
fn test_loss_detector() {
    let mut detector = PacketLossDetector::new();

    // First packet
    let missing = detector.process(100);
    assert!(missing.is_empty());

    // Sequential
    let missing = detector.process(101);
    assert!(missing.is_empty());

    // Gap (102 missing)
    let missing = detector.process(103);
    assert_eq!(missing, vec![102]);

    // Larger gap
    let missing = detector.process(106);
    assert_eq!(missing, vec![104, 105]);
}
