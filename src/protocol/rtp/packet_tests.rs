use super::*;
use crate::protocol::rtp::packet::RtpDecodeError;

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
    let packet = RtpPacket::audio(1, 352, 0xAABBCCDD, payload.clone(), false);

    let encoded = packet.encode();
    let decoded = RtpPacket::decode(&encoded).unwrap();

    assert_eq!(decoded.header.sequence, 1);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn test_payload_type_values() {
    assert_eq!(PayloadType::TimingRequest as u8, 0x52);
    assert_eq!(PayloadType::AudioRealtime as u8, 0x60);
}

#[test]
fn test_decode_invalid_version() {
    let mut buf = [0u8; 12];
    buf[0] = 0x00; // Version 0 instead of 2

    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(0))));
}

#[test]
fn test_audio_samples_iterator() {
    let payload = vec![
        0x00, 0x01, 0x02, 0x03, // Sample 1: L=0x0100, R=0x0302
        0x04, 0x05, 0x06, 0x07, // Sample 2: L=0x0504, R=0x0706
    ];
    let packet = RtpPacket::audio(0, 0, 0, payload, false);

    let samples: Vec<_> = packet.audio_samples().collect();

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0], (0x0100, 0x0302));
    assert_eq!(samples[1], (0x0504, 0x0706));
}

#[test]
fn test_packet_buffer_too_small() {
    let buf = [0u8; 5];
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::BufferTooSmall { .. })));
}

#[test]
fn test_decode_unknown_payload_type() {
    let mut buf = [0u8; 12];
    buf[0] = 0x80; // V=2
    buf[1] = 0xFF; // Unknown PT

    let result = RtpHeader::decode(&buf);
    // The exact error depends on byte & 0x7F = 0x7F
    assert!(matches!(
        result,
        Err(RtpDecodeError::UnknownPayloadType(0x7F))
    ));
}

#[test]
fn test_decode_truncated_packet() {
    let buf = [0u8; 11]; // Less than header size
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::BufferTooSmall { .. })));
}
