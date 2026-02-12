use crate::protocol::rtsp::{RtspCodec, RtspCodecError, StatusCode};

#[test]
fn test_decode_header_too_large() {
    let mut codec = RtspCodec::new().with_max_size(100);
    // Header larger than 100 bytes
    let data = format!("RTSP/1.0 200 OK\r\nLongHeader: {}\r\n\r\n", "a".repeat(100));
    let result = codec.feed(data.as_bytes());
    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}

#[test]
fn test_decode_body_too_large() {
    let mut codec = RtspCodec::new().with_max_size(200);
    let body = "a".repeat(200);
    let data = format!("RTSP/1.0 200 OK\r\nContent-Length: 200\r\n\r\n{body}");

    // Header ~40 bytes + Body 200 bytes > 200 bytes
    let result = codec.feed(data.as_bytes());
    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}

#[test]
fn test_decode_malformed_header_no_colon() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nInvalidHeader\r\n\r\n";
    codec.feed(data).unwrap();
    let result = codec.decode();
    assert!(matches!(result, Err(RtspCodecError::InvalidHeader(_))));
}

#[test]
fn test_decode_malformed_content_length() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nContent-Length: not_a_number\r\n\r\n";
    codec.feed(data).unwrap();
    // Assuming content length parsing failure is caught or defaulted
    // Looking at codec.rs implementation: .content_length().unwrap_or(0)
    // Wait, let's check parse_headers implementation in codec.rs
    // It calls `headers.content_length()`.

    // In `Headers::content_length()`, it does `parse().ok()`.
    // So if parsing fails, it returns None.
    // In `RtspCodec::decode`, `headers.content_length().unwrap_or(0)` is used.
    // So invalid content length becomes 0.

    let result = codec.decode().unwrap().unwrap();
    assert_eq!(result.body.len(), 0);
}

#[test]
fn test_decode_content_length_mismatch_less() {
    // Content-Length says 10, but we only feed 5
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nContent-Length: 10\r\n\r\nhello";
    codec.feed(data).unwrap();
    let result = codec.decode().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_decode_content_length_mismatch_more() {
    // Content-Length says 5, but we feed 10. Should decode first 5 bytes.
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nhelloworld";
    codec.feed(data).unwrap();
    let result = codec.decode().unwrap().unwrap();
    assert_eq!(result.body, b"hello");
    // Remaining bytes are still in buffer?
    // RtspCodec implementation drains buffer.
    // Let's check:
    // "let body = self.buffer.drain(..*content_length).collect();"
    // "self.state = ParseState::StatusLine;"

    // So remaining bytes "world" should be interpreted as start of next response?
    // "world" is not a valid status line.

    let result2 = codec.decode();
    // "world" doesn't have \r\n, so it's incomplete status line
    assert!(matches!(result2, Ok(None)));
}
