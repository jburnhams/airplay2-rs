// tests/raop_rtsp_integration.rs

use airplay2::protocol::raop::{RaopRtspSession, RaopSessionState};
use airplay2::protocol::rtsp::{Headers, Method, RtspResponse, StatusCode};

#[test]
fn test_full_session_flow() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);

    // 1. OPTIONS
    let options = session.options_request();
    assert_eq!(options.method, Method::Options);

    // Simulate successful response
    let mut headers = Headers::new();
    headers.insert("CSeq", "1");
    headers.insert("Public", "ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };

    session
        .process_response(Method::Options, &response)
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::OptionsExchange);

    // 2. ANNOUNCE
    let sdp = session.prepare_announce().unwrap();
    let announce = session.announce_request(&sdp);
    assert_eq!(announce.method, Method::Announce);
    assert!(!announce.body.is_empty());

    // Simulate OK response
    let mut headers = Headers::new();
    headers.insert("CSeq", "2");
    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };
    session
        .process_response(Method::Announce, &response)
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Announcing);

    // 3. SETUP
    let setup = session.setup_request(6001, 6002);
    assert_eq!(setup.method, Method::Setup);

    // Simulate OK response with transport
    let mut headers = Headers::new();
    headers.insert("CSeq", "3");
    headers.insert(
        "Transport",
        "RTP/AVP/UDP;unicast;mode=record;server_port=5000;control_port=5001;timing_port=5002",
    );
    headers.insert("Session", "12345678");
    headers.insert("Audio-Latency", "11025");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };
    session.process_response(Method::Setup, &response).unwrap();
    assert_eq!(session.state(), RaopSessionState::SettingUp);
    assert_eq!(session.session_id(), Some("12345678"));

    let transport = session.transport().unwrap();
    assert_eq!(transport.server_port, 5000);
    assert_eq!(transport.control_port, 5001);

    // 4. RECORD
    let record = session.record_request(100, 2000);
    assert_eq!(record.method, Method::Record);

    // Simulate OK response
    let mut headers = Headers::new();
    headers.insert("CSeq", "4");
    headers.insert("Audio-Latency", "11025");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };
    session.process_response(Method::Record, &response).unwrap();
    assert_eq!(session.state(), RaopSessionState::Recording);

    // 5. TEARDOWN
    let teardown = session.teardown_request();
    assert_eq!(teardown.method, Method::Teardown);

    // Simulate OK response
    let mut headers = Headers::new();
    headers.insert("CSeq", "5");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };
    session
        .process_response(Method::Teardown, &response)
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Terminated);
}
