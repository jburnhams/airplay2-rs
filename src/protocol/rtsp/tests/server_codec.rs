use crate::protocol::rtsp::server_codec::{ParseError, ResponseBuilder, RtspServerCodec};
use crate::protocol::rtsp::{Method, StatusCode};

#[test]
fn test_parse_options_request() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");

    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, Method::Options);
    assert_eq!(request.uri, "*");
    assert_eq!(request.headers.cseq(), Some(1));
}

#[test]
fn test_parse_announce_with_sdp() {
    let sdp = "v=0\r\no=- 0 0 IN IP4 192.168.1.100\r\ns=AirTunes\r\n";
    let request_str = format!(
        "ANNOUNCE rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
         CSeq: 2\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {}",
        sdp.len(),
        sdp
    );

    let mut codec = RtspServerCodec::new();
    codec.feed(request_str.as_bytes());

    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, Method::Announce);
    assert_eq!(request.headers.get("Content-Type"), Some("application/sdp"));
    assert_eq!(String::from_utf8_lossy(&request.body), sdp);
}

#[test]
fn test_parse_incomplete_request() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\n");

    // Should return None (incomplete)
    assert!(codec.decode().unwrap().is_none());

    // Add rest of headers
    codec.feed(b"CSeq: 1\r\n\r\n");

    // Now should parse
    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, Method::Options);
}

#[test]
fn test_parse_incomplete_body() {
    let mut codec = RtspServerCodec::new();
    codec.feed(
        b"SET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 5\r\n\
          Content-Length: 20\r\n\
          \r\n\
          volume: -1", // Only 10 bytes, need 20
    );

    // Should return None (incomplete body)
    assert!(codec.decode().unwrap().is_none());

    // Add rest of body
    codec.feed(b"5.000000\r\n");

    let request = codec.decode().unwrap().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&request.body),
        "volume: -15.000000\r\n"
    );
}

#[test]
fn test_parse_multiple_requests() {
    let mut codec = RtspServerCodec::new();
    codec.feed(
        b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n\
          OPTIONS * RTSP/1.0\r\nCSeq: 2\r\n\r\n",
    );

    let req1 = codec.decode().unwrap().unwrap();
    assert_eq!(req1.headers.cseq(), Some(1));

    let req2 = codec.decode().unwrap().unwrap();
    assert_eq!(req2.headers.cseq(), Some(2));

    // No more requests
    assert!(codec.decode().unwrap().is_none());
}

#[test]
fn test_response_builder() {
    let response = ResponseBuilder::ok()
        .cseq(5)
        .session("ABC123")
        .header("Custom-Header", "value")
        .encode();

    let response_str = String::from_utf8(response).unwrap();
    assert!(response_str.starts_with("RTSP/1.0 200 OK\r\n"));
    assert!(response_str.contains("CSeq: 5\r\n"));
    assert!(response_str.contains("Session: ABC123\r\n"));
    assert!(response_str.contains("Custom-Header: value\r\n"));
    assert!(response_str.ends_with("\r\n\r\n"));
}

#[test]
fn test_response_with_body() {
    let body = "volume: -15.000000\r\n";
    let response = ResponseBuilder::ok().cseq(10).text_body(body).encode();

    let response_str = String::from_utf8(response).unwrap();
    assert!(response_str.contains(&format!("Content-Length: {}\r\n", body.len())));
    assert!(response_str.contains("Content-Type: text/parameters\r\n"));
    assert!(response_str.ends_with(body));
}

#[test]
fn test_error_response() {
    let response = ResponseBuilder::error(StatusCode::NOT_FOUND)
        .cseq(99)
        .encode();

    let response_str = String::from_utf8(response).unwrap();
    assert!(response_str.starts_with("RTSP/1.0 404 Not Found\r\n"));
}

#[test]
fn test_header_overflow() {
    let mut codec = RtspServerCodec::new();
    // Fill buffer with junk until it exceeds limit
    // MAX_HEADER_SIZE is 64KB
    let junk = "X".repeat(65536 + 1);
    codec.feed(junk.as_bytes());

    let result = codec.decode();
    assert!(result.is_err());
    match result {
        Err(ParseError::InvalidHeader(msg)) => {
            assert_eq!(msg, "Headers too large");
        }
        _ => panic!("Expected InvalidHeader error"),
    }
}

#[test]
fn test_body_too_large() {
    let mut codec = RtspServerCodec::new();
    // MAX_BODY_SIZE is 16MB
    let content_length = 16 * 1024 * 1024 + 1;
    let request = format!(
        "SET_PARAMETER rtsp://example.com RTSP/1.0\r\n\
         Content-Length: {content_length}\r\n\r\n"
    );
    codec.feed(request.as_bytes());

    let result = codec.decode();
    assert!(result.is_err());
    match result {
        Err(ParseError::BodyTooLarge { size, max }) => {
            assert_eq!(size, content_length);
            assert_eq!(max, 16 * 1024 * 1024);
        }
        _ => panic!("Expected BodyTooLarge error"),
    }
}

#[test]
fn test_invalid_content_length() {
    let mut codec = RtspServerCodec::new();
    let request = "SET_PARAMETER rtsp://example.com RTSP/1.0\r\n\
                   Content-Length: not_a_number\r\n\r\n";
    codec.feed(request.as_bytes());

    let result = codec.decode();
    assert!(result.is_err());
    match result {
        Err(ParseError::InvalidContentLength(_)) => {}
        _ => panic!("Expected InvalidContentLength error"),
    }
}

#[test]
fn test_malformed_headers() {
    let mut codec = RtspServerCodec::new();
    // Missing colon
    let request = "OPTIONS * RTSP/1.0\r\nInvalidHeader\r\n\r\n";
    codec.feed(request.as_bytes());

    let result = codec.decode();
    assert!(result.is_err());
    match result {
        Err(ParseError::InvalidHeader(line)) => {
            assert_eq!(line, "InvalidHeader");
        }
        _ => panic!("Expected InvalidHeader error"),
    }
}

#[test]
fn test_response_builder_binary_body() {
    let body = vec![0x00, 0x01, 0x02, 0x03];
    let response = ResponseBuilder::ok()
        .binary_body(body.clone(), "application/octet-stream")
        .encode();

    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("Content-Type: application/octet-stream\r\n"));
    assert!(response_str.contains("Content-Length: 4\r\n"));
    assert!(response.ends_with(&body));
}

#[test]
fn test_response_builder_audio_latency() {
    let response = ResponseBuilder::ok().audio_latency(44100).encode();

    let response_str = String::from_utf8(response).unwrap();
    assert!(response_str.contains("Audio-Latency: 44100\r\n"));
}

#[test]
fn test_codec_clear() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
    let _ = codec.decode();

    assert!(codec.buffer_len() == 0);

    codec.feed(b"PARTIAL");
    assert!(codec.buffer_len() > 0);

    codec.clear();
    assert_eq!(codec.buffer_len(), 0);
}

#[test]
fn test_mixed_case_headers() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\ncSEQ: 1\r\n\r\n");
    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.headers.cseq(), Some(1));
}

#[test]
fn test_feed_splitting() {
    let mut codec = RtspServerCodec::new();
    let data = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";

    // Split into 1-byte chunks
    for b in data {
        codec.feed(&[*b]);
    }

    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, Method::Options);
}

#[test]
fn test_unsupported_chunked_encoding() {
    let mut codec = RtspServerCodec::new();
    // Assuming implementation doesn't support chunked, it might treat it as 0 length body if Content-Length is missing
    let request = "SET_PARAMETER * RTSP/1.0\r\n\
                   Transfer-Encoding: chunked\r\n\r\n\
                   5\r\nhello\r\n0\r\n\r\n";
    codec.feed(request.as_bytes());

    let decoded = codec.decode().unwrap().unwrap();
    // Should have parsed headers but body empty because no content-length
    assert!(decoded.body.is_empty());

    // The remaining bytes are still in buffer and will cause error on next decode
    let result = codec.decode();
    assert!(result.is_err());
}

#[test]
fn test_invalid_request_line_empty() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"\r\n\r\n");
    let result = codec.decode();
    assert!(result.is_err());
    assert!(matches!(result, Err(ParseError::InvalidRequestLine(_))));
}

#[test]
fn test_invalid_request_line_too_short() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS\r\n\r\n");
    let result = codec.decode();
    assert!(result.is_err());
    assert!(matches!(result, Err(ParseError::InvalidRequestLine(_))));
}

#[test]
fn test_invalid_protocol_version() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * HTTP/1.1\r\n\r\n");
    let result = codec.decode();
    assert!(result.is_err());
    // Implementation checks for "RTSP/" prefix
    assert!(matches!(result, Err(ParseError::InvalidRequestLine(_))));
}

#[test]
fn test_feed_splitting_edge_case() {
    let mut codec = RtspServerCodec::new();
    codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r");
    // Not enough for header end yet
    assert!(codec.decode().unwrap().is_none());

    codec.feed(b"\n\r\n");
    // Now complete
    let request = codec.decode().unwrap().unwrap();
    assert_eq!(request.method, Method::Options);
}
