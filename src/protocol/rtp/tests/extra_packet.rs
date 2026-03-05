use crate::protocol::rtp::{RtpDecodeError, RtpHeader};

#[test]
fn test_decode_version_zero() {
    let buf = [0x00; 12]; // Version 0
    let result = RtpHeader::decode(&buf);
    assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(0))));
}

#[test]
fn test_decode_unknown_payload_type() {
    let buf = [
        0x80, 0x7F, 0x00, 0x01, // V=2, PT=127 (Unknown)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let result = RtpHeader::decode(&buf);
    assert!(matches!(
        result,
        Err(RtpDecodeError::UnknownPayloadType(127))
    ));
}

#[test]
fn test_decode_extension_bit_set() {
    // If extension bit is set (bit 4 of byte 0), current implementation sets flag but doesn't parse
    // extension header The header decode should succeed, and extension flag should be true.
    let buf = [
        0x90, 0x60, 0x00, 0x01, // V=2, X=1, PT=96 (0x60)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let header = RtpHeader::decode(&buf).unwrap();
    assert!(header.extension);
}
