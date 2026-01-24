use airplay2::protocol::rtsp::{
    Headers, Method, RtspResponse, RtspSession, SessionState, StatusCode,
};

fn simple_response(cseq: u32, session_id: Option<&str>) -> RtspResponse {
    let mut headers = Headers::new();
    headers.insert("CSeq", cseq.to_string());
    if let Some(sid) = session_id {
        headers.insert("Session", sid.to_string());
    }

    RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: vec![],
    }
}

#[test]
fn test_session_restart() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    // 1. Start Session
    let _ = session.options_request();
    let resp = simple_response(1, None);
    session.process_response(Method::Options, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Ready);

    let _ = session.setup_stream_request("transport");
    let resp = simple_response(2, Some("SESSION-1"));
    session.process_response(Method::Setup, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Setup);
    assert_eq!(session.session_id(), Some("SESSION-1"));

    // 2. Teardown
    let _ = session.teardown_request();
    let resp = simple_response(3, None);
    session.process_response(Method::Teardown, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Terminated);

    // 3. Restart (New Session) - Reuse existing session object

    let _ = session.options_request();
    let resp = simple_response(4, None);

    session.process_response(Method::Options, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Ready);

    // Setup new session
    let _ = session.setup_stream_request("transport");
    let resp = simple_response(5, Some("SESSION-2"));
    session.process_response(Method::Setup, &resp).unwrap();
    assert_eq!(session.state(), SessionState::Setup);
    assert_eq!(session.session_id(), Some("SESSION-2"));
}

#[test]
fn test_flush_sequence() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    // Get to Playing state
    let resp = simple_response(0, None);
    session.process_response(Method::Options, &resp).unwrap();
    let resp = simple_response(0, Some("SID"));
    session.process_response(Method::Setup, &resp).unwrap();
    let resp = simple_response(0, None);
    session.process_response(Method::Record, &resp).unwrap();

    assert_eq!(session.state(), SessionState::Playing);

    // Send Flush
    let flush_req = session.flush_request(100, 5000);
    assert!(flush_req.headers.get("RTP-Info").is_some());
    let rtp_info = flush_req.headers.get("RTP-Info").unwrap();
    assert!(rtp_info.contains("seq=100"));
    assert!(rtp_info.contains("rtptime=5000"));

    let resp = simple_response(0, None);
    session.process_response(Method::Flush, &resp).unwrap();

    // Should still be playing
    assert_eq!(session.state(), SessionState::Playing);
}
