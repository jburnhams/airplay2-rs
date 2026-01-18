use super::codec::{RtspCodec, RtspCodecError};
use super::headers::names;
use super::*;

// --- codec.rs tests ---

#[test]
fn test_decode_simple_response() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\n\
                 CSeq: 1\r\n\
                 \r\n",
        )
        .unwrap();

    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.version, "RTSP/1.0");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.reason, "OK");
    assert_eq!(response.cseq(), Some(1));
    assert!(response.body.is_empty());
}

#[test]
fn test_decode_response_with_body() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\n\
                 CSeq: 2\r\n\
                 Content-Length: 5\r\n\
                 \r\n\
                 hello",
        )
        .unwrap();

    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.body, b"hello");
}

#[test]
fn test_decode_incremental() {
    let mut codec = RtspCodec::new();

    // Feed partial data
    codec.feed(b"RTSP/1.0 200 ").unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(b"OK\r\n").unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(b"CSeq: 1\r\n\r\n").unwrap();
    assert!(codec.decode().unwrap().is_some());
}

#[test]
fn test_decode_multiple_responses() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n\
                 RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n",
        )
        .unwrap();

    let r1 = codec.decode().unwrap().unwrap();
    assert_eq!(r1.cseq(), Some(1));

    let r2 = codec.decode().unwrap().unwrap();
    assert_eq!(r2.cseq(), Some(2));

    assert!(codec.decode().unwrap().is_none());
}

#[test]
fn test_decode_invalid_status_line() {
    let mut codec = RtspCodec::new();

    codec.feed(b"INVALID LINE\r\n\r\n").unwrap();

    let result = codec.decode();
    assert!(matches!(result, Err(RtspCodecError::InvalidStatusLine(_))));
}

#[test]
fn test_status_code_checks() {
    assert!(StatusCode::OK.is_success());
    assert!(!StatusCode::OK.is_client_error());

    assert!(StatusCode::NOT_FOUND.is_client_error());
    assert!(!StatusCode::NOT_FOUND.is_success());

    assert!(StatusCode::INTERNAL_ERROR.is_server_error());
}

#[test]
fn test_max_size_limit() {
    let mut codec = RtspCodec::new().with_max_size(100);

    let result = codec.feed(&[0u8; 200]);

    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}

#[test]
fn test_decode_byte_by_byte() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n";

    let mut response = None;
    for byte in data {
        codec.feed(&[*byte]).unwrap();
        if let Some(r) = codec.decode().unwrap() {
            response = Some(r);
            break;
        }
    }

    assert!(response.is_some());
    assert_eq!(response.unwrap().cseq(), Some(1));
}

#[test]
fn test_decode_split_body() {
    let mut codec = RtspCodec::new();
    let header = b"RTSP/1.0 200 OK\r\nContent-Length: 5\r\n\r\n";
    let body_part1 = b"he";
    let body_part2 = b"llo";

    codec.feed(header).unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(body_part1).unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(body_part2).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.body, b"hello");
}

#[test]
fn test_header_case_insensitivity() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nCONTENT-LENGTH: 0\r\ncseq: 99\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.cseq(), Some(99));
    assert_eq!(response.headers.content_length(), Some(0));
}

#[test]
fn test_decode_incomplete_header() {
    let mut codec = RtspCodec::new();
    codec.feed(b"RTSP/1.0 200 OK\r\nContent-Len").unwrap();
    assert!(codec.decode().unwrap().is_none());
}

#[test]
fn test_decode_reset() {
    let mut codec = RtspCodec::new();
    codec.feed(b"RTSP/1.0 200 OK").unwrap(); // Incomplete
    codec.reset();
    assert_eq!(codec.buffered_len(), 0);

    // Should be able to decode fresh packet
    codec.feed(b"RTSP/1.0 200 OK\r\n\r\n").unwrap();
    assert!(codec.decode().unwrap().is_some());
}

// --- session.rs tests ---

#[test]
fn test_session_initial_state() {
    let session = RtspSession::new("192.168.1.10", 7000);

    assert_eq!(session.state(), SessionState::Init);
    assert!(session.session_id().is_none());
}

#[test]
fn test_session_cseq_increments() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    let r1 = session.options_request();
    let r2 = session.options_request();

    assert_eq!(r1.headers.cseq(), Some(1));
    assert_eq!(r2.headers.cseq(), Some(2));
}

#[test]
fn test_session_state_transitions() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    // Initial -> Ready via OPTIONS
    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    };

    session
        .process_response(Method::Options, &response)
        .unwrap();
    assert_eq!(session.state(), SessionState::Ready);
}

#[test]
fn test_session_extracts_session_id() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    let mut headers = Headers::new();
    headers.insert("Session", "ABC123;timeout=60");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };

    session.process_response(Method::Setup, &response).unwrap();

    assert_eq!(session.session_id(), Some("ABC123"));
}

#[test]
fn test_session_can_send_validation() {
    let session = RtspSession::new("192.168.1.10", 7000);

    // In Init state
    assert!(session.can_send(Method::Options));
    assert!(!session.can_send(Method::Setup));
    assert!(!session.can_send(Method::Record));
}

#[test]
fn test_request_includes_common_headers() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let request = session.options_request();

    assert!(request.headers.get("X-Apple-Device-ID").is_some());
    assert!(request.headers.get("X-Apple-Session-ID").is_some());
    assert!(request.headers.get("User-Agent").is_some());
}

#[test]
fn test_invalid_state_transitions() {
    let session = RtspSession::new("192.168.1.10", 7000);

    // Cannot send SETUP before OPTIONS
    assert!(!session.can_send(Method::Setup));

    // Cannot send RECORD before SETUP
    assert!(!session.can_send(Method::Record));
}

#[test]
fn test_process_response_error() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::INTERNAL_ERROR,
        reason: "Internal Error".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    };

    // Should return error
    let result = session.process_response(Method::Options, &response);
    assert!(result.is_err());

    // State should not change on error
    assert_eq!(session.state(), SessionState::Init);
}

#[test]
fn test_teardown_always_allowed() {
    let session = RtspSession::new("192.168.1.10", 7000);
    assert!(session.can_send(Method::Teardown));
}

// --- request.rs tests ---

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

// --- response.rs tests ---

#[test]
fn test_status_code_classification() {
    assert!(StatusCode(200).is_success());
    assert!(StatusCode(201).is_success());
    assert!(!StatusCode(200).is_client_error());
    assert!(!StatusCode(200).is_server_error());

    assert!(StatusCode(400).is_client_error());
    assert!(StatusCode(404).is_client_error());
    assert!(!StatusCode(404).is_success());

    assert!(StatusCode(500).is_server_error());
    assert!(StatusCode(503).is_server_error());
    assert!(!StatusCode(500).is_success());
}

#[test]
fn test_response_is_plist() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/x-apple-binary-plist");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: vec![],
    };

    assert!(response.is_plist());

    let mut headers2 = Headers::new();
    headers2.insert("Content-Type", "text/plain");
    let response2 = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: headers2,
        body: vec![],
    };
    assert!(!response2.is_plist());
}

#[test]
fn test_response_body_as_plist() {
    use crate::protocol::plist::PlistValue;

    // Create a simple plist
    let value = PlistValue::String("test".to_string());
    let encoded = crate::protocol::plist::encode(&value).unwrap();

    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/x-apple-binary-plist");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: encoded,
    };

    let decoded = response.body_as_plist().unwrap();
    assert_eq!(decoded.as_str(), Some("test"));
}

#[test]
fn test_all_state_transitions() {
    let mut session = RtspSession::new("1.2.3.4", 1234);

    // Init -> Ready (OPTIONS)
    let resp = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: Headers::new(),
        body: vec![],
    };
    session.process_response(Method::Options, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Ready);

    // Ready -> Setup (SETUP)
    session.process_response(Method::Setup, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Setup);

    // Setup -> Playing (RECORD)
    session.process_response(Method::Record, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Playing);

    // Playing -> Paused (PAUSE)
    session.process_response(Method::Pause, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Paused);

    // Paused -> Playing (PLAY)
    session.process_response(Method::Play, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Playing);

    // Playing -> Terminated (TEARDOWN)
    session.process_response(Method::Teardown, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Terminated);
}

#[test]
fn test_can_send_comprehensive() {
    let mut session = RtspSession::new("1.2.3.4", 1234);

    // Init
    assert!(session.can_send(Method::Options));
    assert!(session.can_send(Method::Post));
    assert!(!session.can_send(Method::Setup));
    assert!(!session.can_send(Method::Record));
    assert!(session.can_send(Method::Teardown)); // Always allowed

    // Move to Ready
    let resp = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: Headers::new(),
        body: vec![],
    };
    session.process_response(Method::Options, &resp).unwrap();

    // Ready
    assert!(session.can_send(Method::Setup));
    assert!(session.can_send(Method::Post));
    assert!(!session.can_send(Method::Record));

    // Move to Setup
    session.process_response(Method::Setup, &resp).unwrap();

    // Setup
    assert!(session.can_send(Method::Record));
    assert!(session.can_send(Method::Play));
    assert!(!session.can_send(Method::Pause)); // Need to be playing first

    // Move to Playing
    session.process_response(Method::Record, &resp).unwrap();

    // Playing
    assert!(session.can_send(Method::Pause));
    assert!(session.can_send(Method::Flush));
    assert!(session.can_send(Method::SetParameter));
    assert!(session.can_send(Method::GetParameter));
    assert!(session.can_send(Method::Teardown));

    // Move to Paused
    session.process_response(Method::Pause, &resp).unwrap();

    // Paused
    assert!(session.can_send(Method::Record)); // Resume
    assert!(session.can_send(Method::Play)); // Resume
    assert!(session.can_send(Method::Teardown));
    assert!(session.can_send(Method::SetParameter));
}
