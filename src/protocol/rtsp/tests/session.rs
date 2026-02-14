use crate::protocol::rtsp::{Headers, Method, RtspResponse, RtspSession, SessionState, StatusCode};

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

#[test]
fn test_announce_request_body() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let sdp = "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\n";
    let request = session.announce_request(sdp);

    assert_eq!(request.method, Method::Announce);
    assert_eq!(
        request.headers.get("Content-Type").unwrap(),
        "application/sdp"
    );
    assert_eq!(request.body, sdp.as_bytes());
}

#[test]
fn test_setup_stream_request_header() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let transport = "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record";
    let request = session.setup_stream_request(transport);

    assert_eq!(request.method, Method::Setup);
    assert_eq!(request.uri, "/rtp/audio");
    assert_eq!(request.headers.get("Transport").unwrap(), transport);
}

#[test]
fn test_record_request_headers() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    // Move to Setup state first
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
    session.process_response(Method::Setup, &response).unwrap();

    let request = session.record_request();

    assert_eq!(request.method, Method::Record);
}
