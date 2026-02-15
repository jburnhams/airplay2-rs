use crate::protocol::rtp::{ControlPacket, RetransmitRequest, RtpDecodeError};

#[test]
fn test_retransmit_request_encode() {
    let req = RetransmitRequest::new(100, 5);
    let ssrc = 0x1234_5678;

    let encoded = req.encode(ssrc);

    assert_eq!(encoded.len(), 16);

    // Header
    assert_eq!(encoded[0], 0x80);
    assert_eq!(encoded[1], 0xD5); // 0x55 | 0x80

    // Sequence start
    assert_eq!(encoded[2], 0x00);
    assert_eq!(encoded[3], 0x64); // 100

    // Timestamp (0)
    assert_eq!(encoded[4..8], [0, 0, 0, 0]);

    // SSRC
    assert_eq!(encoded[8..12], [0x12, 0x34, 0x56, 0x78]);

    // Payload: sequence start
    assert_eq!(encoded[12], 0x00);
    assert_eq!(encoded[13], 0x64); // 100

    // Payload: count
    assert_eq!(encoded[14], 0x00);
    assert_eq!(encoded[15], 0x05); // 5
}

#[test]
fn test_retransmit_request_decode() {
    let buf = [0x00, 0x64, 0x00, 0x05]; // seq=100, count=5

    let req = RetransmitRequest::decode(&buf).unwrap();
    assert_eq!(req.sequence_start, 100);
    assert_eq!(req.count, 5);
}

#[test]
fn test_control_packet_decode_retransmit() {
    let buf = vec![
        0x80, 0x55, 0x00, 0x00, // Header
        0x00, 0x00, 0x00, 0x00, // Timestamp
        0x12, 0x34, 0x56, 0x78, // SSRC
        0x00, 0x64, 0x00, 0x05, // Payload (RetransmitRequest)
    ];

    let packet = ControlPacket::decode(&buf).unwrap();

    if let ControlPacket::RetransmitRequest(req) = packet {
        assert_eq!(req.sequence_start, 100);
        assert_eq!(req.count, 5);
    } else {
        panic!("Expected RetransmitRequest");
    }
}

#[test]
fn test_control_packet_decode_sync() {
    let buf = vec![
        0x80, 0x54, 0x00, 0x00, // Header (PT=0x54)
        // RTP timestamp (u32)
        0x00, 0x00, 0xAA, 0xBB, // 43707
        // NTP timestamp (u64)
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, // Next timestamp (u32)
        0x00, 0x00, 0xCC, 0xDD, // 52445
    ];

    let packet = ControlPacket::decode(&buf).unwrap();

    if let ControlPacket::Sync {
        rtp_timestamp,
        ntp_timestamp,
        next_timestamp,
    } = packet
    {
        assert_eq!(rtp_timestamp, 43707);
        assert_eq!(ntp_timestamp.seconds, 0x1122_3344);
        assert_eq!(ntp_timestamp.fraction, 0x5566_7788);
        assert_eq!(next_timestamp, 52445);
    } else {
        panic!("Expected Sync packet");
    }
}

#[test]
fn test_control_packet_buffer_too_small() {
    let buf = [0u8; 5];
    let result = ControlPacket::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::BufferTooSmall { .. })));
}

#[test]
fn test_control_packet_unknown_payload() {
    let buf = [
        0x80, 0x99, 0x00, 0x00, // PT=0x99 & 0x7F = 0x19 (25)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let result = ControlPacket::decode(&buf);
    assert!(matches!(
        result,
        Err(RtpDecodeError::UnknownPayloadType(0x19))
    ));
}
