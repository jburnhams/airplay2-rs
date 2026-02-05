use crate::protocol::rtp::{PayloadType, RtpDecodeError, RtpHeader, RtpPacket};

#[test]
fn test_header_encode_decode() {
    let header = RtpHeader::new_audio(100, 44100, 0x12345678, false);

    let encoded = header.encode();
    let decoded = RtpHeader::decode(&encoded).unwrap();

    assert_eq!(decoded.version, 2);
    assert_eq!(decoded.sequence, 100);
    assert_eq!(decoded.timestamp, 44100);
    assert_eq!(decoded.ssrc, 0x12345678);
    assert!(decoded.marker);
}

#[test]
fn test_packet_encode_decode() {
    let payload = vec![0x01, 0x02, 0x03, 0x04];
    let header = RtpHeader::new_audio(101, 44100, 0x12345678, false);
    let packet = RtpPacket::new(header, payload.clone());

    let encoded = packet.encode();
    let decoded = RtpPacket::decode(&encoded).unwrap();

    assert_eq!(decoded.header.sequence, 101);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn test_payload_type_parsing() {
    assert_eq!(
        PayloadType::from_byte(0x60),
        Some(PayloadType::AudioRealtime)
    );
    assert_eq!(
        PayloadType::from_byte(0xE0),
        Some(PayloadType::AudioRealtime)
    ); // Masked
    assert_eq!(
        PayloadType::from_byte(0x56),
        Some(PayloadType::RetransmitResponse)
    );
    assert_eq!(PayloadType::from_byte(0xFF), None); // Unknown
}

#[test]
fn test_buffer_too_small() {
    let buf = [0u8; 5];
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::BufferTooSmall { .. })));
}

#[test]
fn test_invalid_version() {
    // Version 1 (bits 6-7 = 01)
    let buf = [
        0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(1))));
}

#[test]
fn test_audio_samples_iterator() {
    // 4 bytes = 1 sample (2 channels * 16 bit)
    // Left: 0x0102 (258), Right: 0x0304 (772)
    // Little endian: 02 01 04 03
    let payload = vec![0x02, 0x01, 0x04, 0x03];
    let packet = RtpPacket::new(RtpHeader::new_audio(0, 0, 0, false), payload);

    let samples: Vec<(i16, i16)> = packet.audio_samples().collect();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0], (258, 772));
}
