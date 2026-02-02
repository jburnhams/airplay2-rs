use super::codec::{RtspCodec, RtspCodecError};

#[test]
fn test_parse_empty_header_value() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nEmpty-Header: \r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    // Should be empty string
    assert_eq!(response.headers.get("Empty-Header"), Some(""));
}

#[test]
fn test_parse_header_with_lots_of_whitespace() {
    let mut codec = RtspCodec::new();
    // RTSP spec (like HTTP) allows whitespace after colon
    let data = b"RTSP/1.0 200 OK\r\nSpaced-Header:      Value      \r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    // Value should be trimmed
    assert_eq!(response.headers.get("Spaced-Header"), Some("Value"));
}

#[test]
fn test_parse_duplicate_headers_overwrite() {
    // Current implementation overwrites. Testing to enforce this behavior.
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nDup: Value1\r\nDup: Value2\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.headers.get("Dup"), Some("Value2"));
}

#[test]
fn test_parse_header_no_colon() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nInvalidHeaderLine\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    // This should fail according to spec
    let result = codec.decode();
    assert!(matches!(result, Err(RtspCodecError::InvalidHeader(_))));
}

#[test]
fn test_parse_header_colon_only() {
    let mut codec = RtspCodec::new();
    // ": Value" -> Name is empty string?
    let data = b"RTSP/1.0 200 OK\r\n: Value\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    // Implementation likely trims split parts.
    // split_once(':') gives "" and " Value"
    assert_eq!(response.headers.get(""), Some("Value"));
}

#[test]
fn test_parse_header_colon_at_end() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nKey:\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.headers.get("Key"), Some(""));
}

#[test]
fn test_parse_huge_header_value() {
    let mut codec = RtspCodec::new();
    let big_value = "a".repeat(2048);
    let data = format!("RTSP/1.0 200 OK\r\nBig: {big_value}\r\nCSeq: 1\r\n\r\n");

    codec.feed(data.as_bytes()).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.headers.get("Big"), Some(big_value.as_str()));
}

#[test]
fn test_parse_mixed_case_header_names() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nCoNtEnT-TyPe: text/plain\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.headers.get("content-type"), Some("text/plain"));
}

#[test]
fn test_parse_continuation_lines_not_supported() {
    // RTSP 1.0 does not explicitly require support for folded headers (obsolete in HTTP/1.1)
    // Most implementations treat a line starting with space as a new header line or invalid.
    // Let's verify our parser behavior: it likely treats it as an invalid header (no colon).
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nHeader: Value\r\n Continued\r\nCSeq: 1\r\n\r\n";

    codec.feed(data).unwrap();
    let result = codec.decode();

    // " Continued" has no colon, so it returns InvalidHeader
    assert!(matches!(result, Err(RtspCodecError::InvalidHeader(_))));
}
