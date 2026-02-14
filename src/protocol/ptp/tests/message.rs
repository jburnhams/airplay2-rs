use crate::protocol::ptp::message::*;
use crate::protocol::ptp::timestamp::PtpTimestamp;

// ===== PtpMessageType =====

#[test]
fn test_message_type_from_nibble_sync() {
    assert_eq!(
        PtpMessageType::from_nibble(0x00).unwrap(),
        PtpMessageType::Sync
    );
}

#[test]
fn test_message_type_from_nibble_delay_req() {
    assert_eq!(
        PtpMessageType::from_nibble(0x01).unwrap(),
        PtpMessageType::DelayReq
    );
}

#[test]
fn test_message_type_from_nibble_follow_up() {
    assert_eq!(
        PtpMessageType::from_nibble(0x08).unwrap(),
        PtpMessageType::FollowUp
    );
}

#[test]
fn test_message_type_from_nibble_delay_resp() {
    assert_eq!(
        PtpMessageType::from_nibble(0x09).unwrap(),
        PtpMessageType::DelayResp
    );
}

#[test]
fn test_message_type_from_nibble_announce() {
    assert_eq!(
        PtpMessageType::from_nibble(0x0B).unwrap(),
        PtpMessageType::Announce
    );
}

#[test]
fn test_message_type_from_nibble_unknown() {
    assert!(PtpMessageType::from_nibble(0x0F).is_err());
}

#[test]
fn test_message_type_from_nibble_masks_upper_bits() {
    // Upper 4 bits should be ignored.
    assert_eq!(
        PtpMessageType::from_nibble(0xF0).unwrap(),
        PtpMessageType::Sync
    );
    assert_eq!(
        PtpMessageType::from_nibble(0xA1).unwrap(),
        PtpMessageType::DelayReq
    );
}

#[test]
fn test_message_type_is_event() {
    assert!(PtpMessageType::Sync.is_event());
    assert!(PtpMessageType::DelayReq.is_event());
    assert!(!PtpMessageType::FollowUp.is_event());
    assert!(!PtpMessageType::DelayResp.is_event());
    assert!(!PtpMessageType::Announce.is_event());
}

#[test]
fn test_message_type_is_general() {
    assert!(!PtpMessageType::Sync.is_general());
    assert!(PtpMessageType::FollowUp.is_general());
    assert!(PtpMessageType::DelayResp.is_general());
    assert!(PtpMessageType::Announce.is_general());
}

#[test]
fn test_message_type_display() {
    assert_eq!(format!("{}", PtpMessageType::Sync), "Sync");
    assert_eq!(format!("{}", PtpMessageType::DelayReq), "Delay_Req");
    assert_eq!(format!("{}", PtpMessageType::FollowUp), "Follow_Up");
    assert_eq!(format!("{}", PtpMessageType::DelayResp), "Delay_Resp");
    assert_eq!(format!("{}", PtpMessageType::Announce), "Announce");
}

// ===== PtpPortIdentity =====

#[test]
fn test_port_identity_encode_decode_roundtrip() {
    let id = PtpPortIdentity::new(0xDEAD_BEEF_CAFE_BABE, 42);
    let encoded = id.encode();
    let decoded = PtpPortIdentity::decode(&encoded).unwrap();
    assert_eq!(id, decoded);
}

#[test]
fn test_port_identity_decode_too_short() {
    let buf = [0u8; 9];
    assert!(PtpPortIdentity::decode(&buf).is_none());
}

#[test]
fn test_port_identity_encode_length() {
    let id = PtpPortIdentity::new(0, 0);
    assert_eq!(id.encode().len(), 10);
}

#[test]
fn test_port_identity_known_bytes() {
    let id = PtpPortIdentity::new(0x0102_0304_0506_0708, 0x0A0B);
    let buf = id.encode();
    assert_eq!(
        buf,
        [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0B]
    );
}

// ===== PtpHeader =====

#[test]
fn test_header_encode_decode_roundtrip() {
    let source = PtpPortIdentity::new(0x1234_5678_9ABC_DEF0, 1);
    let header = PtpHeader::new(PtpMessageType::Sync, source, 42);
    let encoded = header.encode(10); // 10-byte body
    let decoded = PtpHeader::decode(&encoded).unwrap();

    assert_eq!(decoded.message_type, PtpMessageType::Sync);
    assert_eq!(decoded.version, PtpHeader::PTP_VERSION_2);
    assert_eq!(decoded.source_port_identity, source);
    assert_eq!(decoded.sequence_id, 42);
    assert_eq!(decoded.message_length, 44); // 34 header + 10 body
}

#[test]
fn test_header_encode_size() {
    let source = PtpPortIdentity::new(0, 1);
    let header = PtpHeader::new(PtpMessageType::Sync, source, 0);
    let buf = header.encode(0);
    assert_eq!(buf.len(), PtpHeader::SIZE);
}

#[test]
fn test_header_decode_too_short() {
    let buf = [0u8; 33];
    assert!(PtpHeader::decode(&buf).is_err());
}

#[test]
fn test_header_control_field_values() {
    let source = PtpPortIdentity::new(0, 1);

    let sync = PtpHeader::new(PtpMessageType::Sync, source, 0);
    assert_eq!(sync.control_field, 0x00);

    let delay_req = PtpHeader::new(PtpMessageType::DelayReq, source, 0);
    assert_eq!(delay_req.control_field, 0x01);

    let follow_up = PtpHeader::new(PtpMessageType::FollowUp, source, 0);
    assert_eq!(follow_up.control_field, 0x02);

    let delay_resp = PtpHeader::new(PtpMessageType::DelayResp, source, 0);
    assert_eq!(delay_resp.control_field, 0x03);

    let announce = PtpHeader::new(PtpMessageType::Announce, source, 0);
    assert_eq!(announce.control_field, 0x05);
}

#[test]
fn test_header_transport_specific_preserved() {
    let source = PtpPortIdentity::new(0, 1);
    let mut header = PtpHeader::new(PtpMessageType::Sync, source, 0);
    header.transport_specific = 0x05;
    let encoded = header.encode(0);
    let decoded = PtpHeader::decode(&encoded).unwrap();
    assert_eq!(decoded.transport_specific, 0x05);
}

#[test]
fn test_header_flags_preserved() {
    let source = PtpPortIdentity::new(0, 1);
    let mut header = PtpHeader::new(PtpMessageType::Sync, source, 0);
    header.flags = 0x0200; // Two-step flag
    let encoded = header.encode(0);
    let decoded = PtpHeader::decode(&encoded).unwrap();
    assert_eq!(decoded.flags, 0x0200);
}

#[test]
fn test_header_correction_field_preserved() {
    let source = PtpPortIdentity::new(0, 1);
    let mut header = PtpHeader::new(PtpMessageType::Sync, source, 0);
    header.correction_field = 123_456_789;
    let encoded = header.encode(0);
    let decoded = PtpHeader::decode(&encoded).unwrap();
    assert_eq!(decoded.correction_field, 123_456_789);
}

#[test]
fn test_header_sequence_wrapping() {
    let source = PtpPortIdentity::new(0, 1);
    let header = PtpHeader::new(PtpMessageType::Sync, source, u16::MAX);
    let encoded = header.encode(0);
    let decoded = PtpHeader::decode(&encoded).unwrap();
    assert_eq!(decoded.sequence_id, u16::MAX);
}

// ===== PtpMessage (full IEEE 1588) =====

#[test]
fn test_sync_message_roundtrip() {
    let source = PtpPortIdentity::new(0xAABB_CCDD_EEFF_0011, 1);
    let ts = PtpTimestamp::new(1000, 500_000_000);
    let msg = PtpMessage::sync(source, 7, ts);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.header.message_type, PtpMessageType::Sync);
    assert_eq!(decoded.header.sequence_id, 7);
    assert_eq!(decoded.header.source_port_identity, source);
    match decoded.body {
        PtpMessageBody::Sync { origin_timestamp } => {
            assert_eq!(origin_timestamp, ts);
        }
        _ => panic!("Expected Sync body"),
    }
}

#[test]
fn test_follow_up_message_roundtrip() {
    let source = PtpPortIdentity::new(0x1122_3344_5566_7788, 1);
    let ts = PtpTimestamp::new(2000, 123_456_789);
    let msg = PtpMessage::follow_up(source, 12, ts);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.header.message_type, PtpMessageType::FollowUp);
    match decoded.body {
        PtpMessageBody::FollowUp {
            precise_origin_timestamp,
        } => {
            assert_eq!(precise_origin_timestamp, ts);
        }
        _ => panic!("Expected FollowUp body"),
    }
}

#[test]
fn test_delay_req_message_roundtrip() {
    let source = PtpPortIdentity::new(0xDEAD_BEEF_0000_0000, 2);
    let ts = PtpTimestamp::new(3000, 999_999_999);
    let msg = PtpMessage::delay_req(source, 99, ts);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.header.message_type, PtpMessageType::DelayReq);
    assert_eq!(decoded.header.sequence_id, 99);
    match decoded.body {
        PtpMessageBody::DelayReq { origin_timestamp } => {
            assert_eq!(origin_timestamp, ts);
        }
        _ => panic!("Expected DelayReq body"),
    }
}

#[test]
fn test_delay_resp_message_roundtrip() {
    let source = PtpPortIdentity::new(0x1111_1111_1111_1111, 1);
    let requesting = PtpPortIdentity::new(0x2222_2222_2222_2222, 2);
    let ts = PtpTimestamp::new(4000, 0);
    let msg = PtpMessage::delay_resp(source, 50, ts, requesting);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.header.message_type, PtpMessageType::DelayResp);
    match decoded.body {
        PtpMessageBody::DelayResp {
            receive_timestamp,
            requesting_port_identity,
        } => {
            assert_eq!(receive_timestamp, ts);
            assert_eq!(requesting_port_identity, requesting);
        }
        _ => panic!("Expected DelayResp body"),
    }
}

#[test]
fn test_announce_message_roundtrip() {
    let source = PtpPortIdentity::new(0xAAAA_BBBB_CCCC_DDDD, 1);
    let gm_id = 0xEEEE_FFFF_0000_1111;
    let msg = PtpMessage::announce(source, 1, gm_id, 128, 248);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.header.message_type, PtpMessageType::Announce);
    match decoded.body {
        PtpMessageBody::Announce {
            grandmaster_identity,
            grandmaster_priority1,
            grandmaster_priority2,
            ..
        } => {
            assert_eq!(grandmaster_identity, gm_id);
            assert_eq!(grandmaster_priority1, 128);
            assert_eq!(grandmaster_priority2, 248);
        }
        _ => panic!("Expected Announce body"),
    }
}

#[test]
fn test_message_decode_truncated_sync() {
    let source = PtpPortIdentity::new(0, 1);
    let msg = PtpMessage::sync(source, 0, PtpTimestamp::ZERO);
    let encoded = msg.encode();
    // Truncate body.
    let truncated = &encoded[..PtpHeader::SIZE + 5];
    assert!(PtpMessage::decode(truncated).is_err());
}

#[test]
fn test_message_decode_truncated_delay_resp() {
    let source = PtpPortIdentity::new(0, 1);
    let requesting = PtpPortIdentity::new(0, 2);
    let msg = PtpMessage::delay_resp(source, 0, PtpTimestamp::ZERO, requesting);
    let encoded = msg.encode();
    // Truncate - remove last byte of requesting port identity.
    let truncated = &encoded[..encoded.len() - 1];
    assert!(PtpMessage::decode(truncated).is_err());
}

#[test]
fn test_message_decode_empty() {
    assert!(PtpMessage::decode(&[]).is_err());
}

#[test]
fn test_message_encode_sizes() {
    let source = PtpPortIdentity::new(0, 1);
    let requesting = PtpPortIdentity::new(0, 2);

    // Sync: 34 header + 10 body = 44.
    let sync = PtpMessage::sync(source, 0, PtpTimestamp::ZERO);
    assert_eq!(sync.encode().len(), 44);

    // FollowUp: 34 + 10 = 44.
    let fu = PtpMessage::follow_up(source, 0, PtpTimestamp::ZERO);
    assert_eq!(fu.encode().len(), 44);

    // DelayReq: 34 + 10 = 44.
    let dr = PtpMessage::delay_req(source, 0, PtpTimestamp::ZERO);
    assert_eq!(dr.encode().len(), 44);

    // DelayResp: 34 + 20 = 54.
    let drp = PtpMessage::delay_resp(source, 0, PtpTimestamp::ZERO, requesting);
    assert_eq!(drp.encode().len(), 54);

    // Announce: 34 + 20 = 54.
    let ann = PtpMessage::announce(source, 0, 0, 128, 248);
    assert_eq!(ann.encode().len(), 54);
}

// ===== AirPlayTimingPacket =====

#[test]
fn test_airplay_packet_sync_roundtrip() {
    let pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::Sync,
        sequence_id: 100,
        timestamp: PtpTimestamp::new(1000, 0),
        clock_id: 0xDEAD_BEEF_CAFE_BABE,
    };
    let encoded = pkt.encode();
    assert_eq!(encoded.len(), AirPlayTimingPacket::SIZE);

    let decoded = AirPlayTimingPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.message_type, PtpMessageType::Sync);
    assert_eq!(decoded.sequence_id, 100);
    assert_eq!(decoded.clock_id, 0xDEAD_BEEF_CAFE_BABE);
    // Timestamp seconds should match.
    assert_eq!(decoded.timestamp.seconds, 1000);
}

#[test]
fn test_airplay_packet_delay_req_roundtrip() {
    let pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        sequence_id: 0xFFFF,
        timestamp: PtpTimestamp::new(5000, 500_000_000),
        clock_id: 0x0123_4567_89AB_CDEF,
    };
    let encoded = pkt.encode();
    let decoded = AirPlayTimingPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.message_type, PtpMessageType::DelayReq);
    assert_eq!(decoded.sequence_id, 0xFFFF);
    assert_eq!(decoded.timestamp.seconds, 5000);
}

#[test]
fn test_airplay_packet_decode_too_short() {
    let buf = [0u8; 15];
    assert!(AirPlayTimingPacket::decode(&buf).is_err());
}

#[test]
fn test_airplay_packet_message_type_byte() {
    let pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::Sync,
        sequence_id: 0,
        timestamp: PtpTimestamp::ZERO,
        clock_id: 0,
    };
    let encoded = pkt.encode();
    assert_eq!(encoded[0] & 0x0F, 0x00); // Sync

    let pkt2 = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        ..pkt
    };
    let encoded2 = pkt2.encode();
    assert_eq!(encoded2[0] & 0x0F, 0x01); // DelayReq
}

#[test]
fn test_airplay_packet_sequence_id_bytes() {
    let pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::Sync,
        sequence_id: 0x1234,
        timestamp: PtpTimestamp::ZERO,
        clock_id: 0,
    };
    let encoded = pkt.encode();
    assert_eq!(encoded[2], 0x12);
    assert_eq!(encoded[3], 0x34);
}

// ===== PtpParseError =====

#[test]
fn test_parse_error_too_short_display() {
    let err = PtpParseError::TooShort {
        needed: 34,
        have: 10,
    };
    let msg = format!("{err}");
    assert!(msg.contains("34"));
    assert!(msg.contains("10"));
}

#[test]
fn test_parse_error_unknown_type_display() {
    let err = PtpParseError::UnknownMessageType(0x0F);
    let msg = format!("{err}");
    assert!(msg.contains("0F") || msg.contains("0f"));
}
