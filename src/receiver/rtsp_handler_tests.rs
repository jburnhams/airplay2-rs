use super::rtsp_handler::*;
use super::session::{ReceiverSession, SessionState};
use crate::protocol::rtsp::{Headers, Method, RtspRequest, StatusCode};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn test_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345)
}

fn create_request(method: Method) -> RtspRequest {
    RtspRequest {
        method,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    }
}

#[test]
fn test_options() {
    let session = ReceiverSession::new(test_addr());
    let mut request = create_request(Method::Options);
    request.headers.insert("CSeq".to_string(), "1".to_string());

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert!(result.response.headers.contains("Public"));
    assert_eq!(result.response.headers.cseq(), Some(1));
}

#[test]
fn test_announce_valid_state() {
    let session = ReceiverSession::new(test_addr());
    // Default state is Connected, which is valid for ANNOUNCE
    let mut request = create_request(Method::Announce);
    request.headers.insert("CSeq".to_string(), "2".to_string());
    request
        .headers
        .insert("Content-Type".to_string(), "application/sdp".to_string());

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert_eq!(result.new_state, Some(SessionState::Announced));
}

#[test]
fn test_announce_invalid_state() {
    let mut session = ReceiverSession::new(test_addr());
    // Move to streaming state via valid transitions
    session.set_state(SessionState::Announced).unwrap();
    session.set_state(SessionState::Setup).unwrap();
    session.set_state(SessionState::Streaming).unwrap();

    let request = create_request(Method::Announce);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::METHOD_NOT_VALID);
}

#[test]
fn test_setup_valid() {
    let mut session = ReceiverSession::new(test_addr());
    // Connected -> Announced
    session.set_state(SessionState::Announced).unwrap();

    let mut request = create_request(Method::Setup);
    request.headers.insert(
        "Transport".to_string(),
        "RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002".to_string(),
    );

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert!(result.response.headers.contains("Transport"));
    assert!(result.response.headers.contains("Session"));
    assert_eq!(result.new_state, Some(SessionState::Setup));
    assert!(result.allocated_ports.is_some());
}

#[test]
fn test_setup_invalid_transport() {
    let mut session = ReceiverSession::new(test_addr());
    // Connected -> Announced
    session.set_state(SessionState::Announced).unwrap();

    let mut request = create_request(Method::Setup);
    request
        .headers
        .insert("Transport".to_string(), "InvalidTransport".to_string());

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::BAD_REQUEST);
}

#[test]
fn test_record_valid() {
    let mut session = ReceiverSession::new(test_addr());
    // Connected -> Announced -> Setup
    session.set_state(SessionState::Announced).unwrap();
    session.set_state(SessionState::Setup).unwrap();

    let mut request = create_request(Method::Record);
    request
        .headers
        .insert("RTP-Info".to_string(), "seq=1;rtptime=12345".to_string());

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert!(result.response.headers.contains("Audio-Latency"));
    assert_eq!(result.new_state, Some(SessionState::Streaming));
    assert!(result.start_streaming);
}

#[test]
fn test_record_invalid_state() {
    let session = ReceiverSession::new(test_addr());
    // Connected state is invalid for RECORD
    let request = create_request(Method::Record);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::METHOD_NOT_VALID);
}

#[test]
fn test_pause() {
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::Pause);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert_eq!(result.new_state, Some(SessionState::Paused));
}

#[test]
fn test_flush() {
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::Flush);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert!(result.new_state.is_none());
}

#[test]
fn test_teardown() {
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::Teardown);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert_eq!(result.new_state, Some(SessionState::Teardown));
    assert!(result.stop_streaming);
}

#[test]
fn test_get_parameter_empty() {
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::GetParameter);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    assert!(result.response.body.is_empty());
}

#[test]
fn test_get_parameter_volume() {
    let mut session = ReceiverSession::new(test_addr());
    session.set_volume(-15.0);

    let mut request = create_request(Method::GetParameter);
    request.body = b"volume".to_vec();

    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::OK);
    let body = String::from_utf8(result.response.body).unwrap();
    assert!(body.contains("volume: -15.000000"));
}

#[test]
fn test_unknown_method() {
    let session = ReceiverSession::new(test_addr());
    // Using OPTIONS as a placeholder, but treating it as unknown if we force it?
    // Actually `handle_request` matches Method enum.
    // We can use a method that is not implemented, e.g. PLAY
    let request = create_request(Method::Play);
    let result = handle_request(&request, &session);

    assert_eq!(result.response.status, StatusCode::METHOD_NOT_ALLOWED);
}
