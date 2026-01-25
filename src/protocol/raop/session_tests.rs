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
