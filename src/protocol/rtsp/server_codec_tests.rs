use super::server_codec::{ResponseBuilder, RtspServerCodec};
use super::{Method, StatusCode};

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
