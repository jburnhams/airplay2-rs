use super::*;
use crate::protocol::rtsp::{Headers, Method, RtspResponse, StatusCode};

#[test]
fn test_session_creation() {
    let session = RaopRtspSession::new("192.168.1.50", 5000);

    assert_eq!(session.state(), RaopSessionState::Init);
    assert!(session.session_id.is_none());
    assert!(!session.client_instance.is_empty());
}

#[test]
fn test_options_request() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);
    let request = session.options_request();

    assert_eq!(request.method, Method::Options);
    assert!(request.headers.get("Apple-Challenge").is_some());
    assert!(request.headers.get("CSeq").is_some());
    assert!(request.headers.get("Client-Instance").is_some());
}

#[test]
fn test_setup_request() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);
    let request = session.setup_request(6001, 6002);

    assert_eq!(request.method, Method::Setup);
    let transport = request.headers.get("Transport").unwrap();
    assert!(transport.contains("control_port=6001"));
    assert!(transport.contains("timing_port=6002"));
}

#[test]
fn test_transport_parsing() {
    let transport_str =
        "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
    let transport = RaopRtspSession::parse_transport(transport_str).unwrap();

    assert_eq!(transport.server_port, 6000);
    assert_eq!(transport.control_port, 6001);
    assert_eq!(transport.timing_port, 6002);
}

#[test]
fn test_volume_request() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);
    let request = session.set_volume_request(-15.0);

    assert_eq!(request.method, Method::SetParameter);
    let body = String::from_utf8_lossy(&request.body);
    assert!(body.contains("volume:"));
    assert!(body.contains("-15"));
}

#[test]
fn test_process_response_flow() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);

    // OPTIONS
    let mut headers = Headers::new();
    headers.insert("Apple-Response", "test_response");
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

    // ANNOUNCE
    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    };
    session
        .process_response(Method::Announce, &response)
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Announcing);
}

#[test]
fn test_full_state_machine() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);

    // Helper to create OK response
    let ok_response = |headers: Headers| RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: Vec::new(),
    };

    // 1. OPTIONS
    session.options_request();
    let mut headers = Headers::new();
    headers.insert("Apple-Response", "challenge_response");
    session
        .process_response(Method::Options, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::OptionsExchange);

    // 2. ANNOUNCE
    let sdp = session.prepare_announce().unwrap();
    session.announce_request(&sdp);
    let headers = Headers::new();
    session
        .process_response(Method::Announce, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Announcing);

    // 3. SETUP
    session.setup_request(6001, 6002);
    let mut headers = Headers::new();
    headers.insert(
        "Transport",
        "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002",
    );
    headers.insert("Session", "SESSION_ID");
    headers.insert("Audio-Latency", "11025");
    session
        .process_response(Method::Setup, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::SettingUp);
    assert_eq!(session.session_id(), Some("SESSION_ID"));
    assert!(session.transport().is_some());

    // 4. RECORD
    session.record_request(1, 12345);
    let mut headers = Headers::new();
    headers.insert("Audio-Latency", "11025"); // Sometimes echoed
    session
        .process_response(Method::Record, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Recording);

    // 5. FLUSH (Pause)
    session.flush_request(2, 67890);
    let headers = Headers::new();
    session
        .process_response(Method::Flush, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Paused);

    // 6. TEARDOWN
    session.teardown_request();
    let headers = Headers::new();
    session
        .process_response(Method::Teardown, &ok_response(headers))
        .unwrap();
    assert_eq!(session.state(), RaopSessionState::Terminated);
}

#[test]
fn test_parse_transport_missing_fields() {
    // Missing server_port
    let transport_str = "RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002";
    let result = RaopRtspSession::parse_transport(transport_str);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "missing server_port in transport");

    // Valid
    let transport_str = "RTP/AVP/UDP;unicast;mode=record;server_port=6000";
    let result = RaopRtspSession::parse_transport(transport_str);
    assert!(result.is_ok());
    let t = result.unwrap();
    assert_eq!(t.server_port, 6000);
    // Others default to 0
    assert_eq!(t.control_port, 0);
}

#[test]
fn test_process_response_failure() {
    let mut session = RaopRtspSession::new("192.168.1.50", 5000);

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::BAD_REQUEST,
        reason: "Bad Request".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    };

    let result = session.process_response(Method::Options, &response);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("400 Bad Request"));
}
