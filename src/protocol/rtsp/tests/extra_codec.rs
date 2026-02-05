use crate::protocol::rtsp::StatusCode;
use crate::protocol::rtsp::codec::{RtspCodec, RtspCodecError};

#[test]
fn test_decode_invalid_utf8_header_value() {
    let mut codec = RtspCodec::new();
    // 0xFF is invalid UTF-8
    let data = b"RTSP/1.0 200 OK\r\nInvalid: \xFF\r\nCSeq: 1\r\n\r\n";
    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();
    // String::from_utf8_lossy replaces invalid with REPLACEMENT CHARACTER
    assert!(
        response
            .headers
            .get("Invalid")
            .unwrap()
            .contains('\u{FFFD}')
    );
}

#[test]
fn test_decode_header_line_too_long() {
    let mut codec = RtspCodec::new().with_max_size(1024);
    let junk = "A".repeat(1025);
    let data = format!("RTSP/1.0 200 OK\r\nHeader: {}\r\n\r\n", junk);

    let result = codec.feed(data.as_bytes());
    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}

#[test]
fn test_decode_malformed_status_line_missing_reason() {
    let mut codec = RtspCodec::new();
    // "RTSP/1.0 200" without reason phrase (some devices might do this)
    // The parser implementation splits by whitespace.
    // parts: "RTSP/1.0", "200"
    // reason reconstruction: join remaining parts -> empty string.
    let data = b"RTSP/1.0 200\r\nCSeq: 1\r\n\r\n";
    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.reason, "");
}

#[test]
fn test_decode_malformed_status_line_bad_version() {
    let mut codec = RtspCodec::new();
    // Missing version
    let data = b"200 OK\r\nCSeq: 1\r\n\r\n";
    codec.feed(data).unwrap();
    let result = codec.decode();
    // "200" is taken as version, "OK" as status (fails parsing u16)
    assert!(matches!(result, Err(RtspCodecError::InvalidStatusLine(_))));
}
