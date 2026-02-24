use crate::protocol::rtsp::{Method, RtspRequest};

#[test]
fn test_request_encode_simple() {
    let request = RtspRequest::builder(Method::Options, "rtsp://192.168.1.10:7000/*")
        .cseq(1)
        .user_agent("test/1.0")
        .build();

    let encoded = request.encode();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.starts_with("OPTIONS rtsp://192.168.1.10:7000/* RTSP/1.0\r\n"));
    assert!(encoded_str.contains("CSeq: 1\r\n"));
    assert!(encoded_str.contains("User-Agent: test/1.0\r\n"));
    assert!(encoded_str.ends_with("\r\n\r\n"));
}

#[test]
fn test_request_encode_with_body() {
    let body = b"test body content".to_vec();
    let request = RtspRequest::builder(Method::SetParameter, "rtsp://example.com/")
        .cseq(5)
        .content_type("text/parameters")
        .body(body.clone())
        .build();

    let encoded = request.encode();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("Content-Type: text/parameters\r\n"));
    assert!(encoded_str.contains(&format!("Content-Length: {}\r\n", body.len())));
    assert!(encoded_str.ends_with("test body content"));
}

#[test]
fn test_method_as_str() {
    assert_eq!(Method::Options.as_str(), "OPTIONS");
    assert_eq!(Method::Setup.as_str(), "SETUP");
    assert_eq!(Method::SetParameter.as_str(), "SET_PARAMETER");
}

#[test]
fn test_method_from_str() {
    assert_eq!("OPTIONS".parse::<Method>(), Ok(Method::Options));
    assert_eq!("options".parse::<Method>(), Ok(Method::Options));
    assert!("INVALID".parse::<Method>().is_err());
}

#[test]
fn test_request_builder_methods() {
    let request = RtspRequest::builder(Method::Play, "rtsp://test")
        .header("Custom", "Value")
        .build();

    assert_eq!(request.method, Method::Play);
    assert_eq!(request.headers.get("Custom"), Some("Value"));
}

#[test]
fn test_request_builder_defaults() {
    let request = RtspRequest::builder(Method::Options, "*").build();
    assert_eq!(request.method, Method::Options);
    assert_eq!(request.uri, "*");
    assert!(request.headers.is_empty());
    assert!(request.body.is_empty());
}

#[test]
fn test_post_request_encode() {
    let request = RtspRequest::builder(Method::Post, "/pair-setup")
        .cseq(10)
        .content_type("application/x-apple-binary-plist")
        .body(vec![0x00, 0x01, 0x02])
        .build();

    let encoded = request.encode();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.starts_with("POST /pair-setup RTSP/1.0\r\n"));
    assert!(encoded_str.contains("Content-Type: application/x-apple-binary-plist"));
}

#[test]
fn test_get_request_encode() {
    let request = RtspRequest::builder(Method::Get, "/info")
        .cseq(1)
        .header("X-Apple-Device-ID", "00:11:22:33:44:55")
        .build();

    let encoded = request.encode();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.starts_with("GET /info RTSP/1.0\r\n"));
    assert!(encoded_str.contains("X-Apple-Device-ID: 00:11:22:33:44:55"));
}

#[test]
fn test_request_builder_case_insensitive_headers() {
    let request = RtspRequest::builder(Method::Options, "*")
        .header("content-type", "application/json")
        .build();

    // Should find it even with different case
    assert_eq!(
        request.headers.get("CONTENT-TYPE"),
        Some("application/json")
    );
    assert_eq!(
        request.headers.get("Content-Type"),
        Some("application/json")
    );
}

#[test]
fn test_large_body() {
    let body = vec![0u8; 1024 * 1024]; // 1MB
    let request = RtspRequest::builder(Method::Post, "/upload")
        .body(body.clone())
        .build();

    assert_eq!(request.body.len(), 1024 * 1024);
    assert_eq!(request.headers.get("Content-Length"), Some("1048576"));
}
