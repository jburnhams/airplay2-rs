use crate::protocol::rtp::{PayloadType, RtpDecodeError, RtpHeader, RtpPacket};

#[test]
fn test_invalid_version_extended() {
    let mut buf = [0u8; 12];
    // Version 3 (bits 6-7 = 11)
    buf[0] = 0xC0;
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(3))));

    // Version 0 (bits 6-7 = 00)
    buf[0] = 0x00;
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(0))));
}

#[test]
fn test_buffer_too_small_exact_boundaries() {
    // Header size is 12 bytes
    let buf = [0u8; 11];
    let result = RtpHeader::decode(&buf);
    assert!(matches!(
        result,
        Err(RtpDecodeError::BufferTooSmall {
            needed: 12,
            have: 11
        })
    ));

    let buf_ok = [
        0x80, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert!(RtpHeader::decode(&buf_ok).is_ok());
}

#[test]
fn test_unknown_payload_type() {
    let mut buf = [
        0x80, 0xFF, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    // Payload type 0x7F (127) - not defined in PayloadType enum
    buf[1] = 0xFF; // M=1, PT=127

    let result = RtpHeader::decode(&buf);
    assert!(matches!(
        result,
        Err(RtpDecodeError::UnknownPayloadType(127))
    ));
}

#[test]
fn test_packet_decode_exact_payload() {
    // Header (12 bytes) + 4 bytes payload
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&[
        0x80, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    let packet = RtpPacket::decode(&data).unwrap();
    assert_eq!(packet.payload.len(), 4);
    assert_eq!(packet.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn test_packet_decode_empty_payload() {
    // Header only
    let data = [
        0x80, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let packet = RtpPacket::decode(&data).unwrap();
    assert!(packet.payload.is_empty());
}

#[test]
fn test_audio_samples_odd_size() {
    // Payload with 5 bytes (not multiple of 4)
    // Should iterate over first 4 bytes and ignore the last one
    let payload = vec![0x02, 0x01, 0x04, 0x03, 0xFF];
    let packet = RtpPacket::new(RtpHeader::new_audio(0, 0, 0, false), payload);

    let samples: Vec<(i16, i16)> = packet.audio_samples().collect();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0], (258, 772));
}

#[test]
fn test_payload_type_variants() {
    assert_eq!(
        PayloadType::from_byte(0xD2),
        Some(PayloadType::TimingRequest)
    ); // 0x52 | 0x80
    assert_eq!(
        PayloadType::from_byte(0xD3),
        Some(PayloadType::TimingResponse)
    ); // 0x53 | 0x80
    assert_eq!(
        PayloadType::from_byte(0xD5),
        Some(PayloadType::RetransmitRequest)
    ); // 0x55 | 0x80
    assert_eq!(
        PayloadType::from_byte(0xE1),
        Some(PayloadType::AudioBuffered)
    ); // 0x61 | 0x80
}

#[test]
fn test_audio_header_creation() {
    let header_realtime = RtpHeader::new_audio(1, 1000, 12345, false);
    assert_eq!(header_realtime.payload_type, PayloadType::AudioRealtime);

    let header_buffered = RtpHeader::new_audio(2, 2000, 12345, true);
    assert_eq!(header_buffered.payload_type, PayloadType::AudioBuffered);
}

#[test]
fn test_audio_samples_empty() {
    let payload = Vec::new();
    let packet = RtpPacket::new(RtpHeader::new_audio(0, 0, 0, false), payload);
    let samples: Vec<(i16, i16)> = packet.audio_samples().collect();
    assert!(samples.is_empty());
}
