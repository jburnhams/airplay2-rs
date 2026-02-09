use crate::protocol::rtsp::server_codec::{ParseError, RtspServerCodec};

#[test]
fn test_incomplete_data() {
    let mut codec = RtspServerCodec::new();
    // Send part of a request
    codec.feed(b"OPTIONS * RTSP/1.0\r\n");
    assert!(codec.decode().unwrap().is_none());

    // Send the rest
    codec.feed(b"CSeq: 1\r\n\r\n");
    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, crate::protocol::rtsp::Method::Options);
}

#[test]
fn test_malformed_request_line() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"INVALID_REQUEST\r\n\r\n");
    let result = codec.decode();
    assert!(matches!(result, Err(ParseError::InvalidRequestLine(_))));
}

#[test]
fn test_invalid_protocol() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * HTTP/1.1\r\n\r\n");
    let result = codec.decode();
    assert!(matches!(result, Err(ParseError::InvalidRequestLine(_))));
}

#[test]
fn test_invalid_header_format() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\nInvalidHeader\r\n\r\n");
    let result = codec.decode();
    assert!(matches!(result, Err(ParseError::InvalidHeader(_))));
}

#[test]
fn test_body_too_large() {
    let mut codec = RtspServerCodec::new();
    // 17MB content length
    codec.feed(b"ANNOUNCE * RTSP/1.0\r\nContent-Length: 17825792\r\n\r\n");
    let result = codec.decode();
    assert!(matches!(result, Err(ParseError::BodyTooLarge { .. })));
}

#[test]
fn test_invalid_content_length() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"ANNOUNCE * RTSP/1.0\r\nContent-Length: not_a_number\r\n\r\n");
    let result = codec.decode();
    assert!(matches!(result, Err(ParseError::InvalidContentLength(_))));
}

#[test]
fn test_multiple_requests() {
    let mut codec = RtspServerCodec::new();
    let data = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\nSETUP / RTSP/1.0\r\nCSeq: 2\r\n\r\n";
    codec.feed(data);

    let req1 = codec.decode().unwrap().unwrap();
    assert_eq!(req1.method, crate::protocol::rtsp::Method::Options);

    let req2 = codec.decode().unwrap().unwrap();
    assert_eq!(req2.method, crate::protocol::rtsp::Method::Setup);
}
