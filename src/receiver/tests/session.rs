use crate::receiver::session::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn test_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

#[test]
fn test_session_initial_state() {
    let session = ReceiverSession::new(test_addr());
    assert_eq!(session.state(), SessionState::Connected);
    assert_eq!(session.client_addr(), test_addr());
    assert!((session.volume() - 0.0).abs() < f32::EPSILON);
    assert!(!session.id().is_empty());
}

#[test]
fn test_valid_state_transitions() {
    let mut session = ReceiverSession::new(test_addr());

    // Connected -> Announced
    assert!(session.set_state(SessionState::Announced).is_ok());
    assert_eq!(session.state(), SessionState::Announced);

    // Announced -> Setup
    assert!(session.set_state(SessionState::Setup).is_ok());
    assert_eq!(session.state(), SessionState::Setup);

    // Setup -> Streaming
    assert!(session.set_state(SessionState::Streaming).is_ok());
    assert_eq!(session.state(), SessionState::Streaming);

    // Streaming -> Paused
    assert!(session.set_state(SessionState::Paused).is_ok());
    assert_eq!(session.state(), SessionState::Paused);

    // Paused -> Streaming
    assert!(session.set_state(SessionState::Streaming).is_ok());

    // Streaming -> Teardown
    assert!(session.set_state(SessionState::Teardown).is_ok());
    assert_eq!(session.state(), SessionState::Teardown);

    // Teardown -> Closed
    assert!(session.set_state(SessionState::Closed).is_ok());
    assert_eq!(session.state(), SessionState::Closed);
}

#[test]
fn test_invalid_state_transitions() {
    let mut session = ReceiverSession::new(test_addr());

    // Connected -> Streaming (invalid)
    assert!(session.set_state(SessionState::Streaming).is_err());
    assert_eq!(session.state(), SessionState::Connected);

    // Connected -> Setup (invalid, needs Announce)
    assert!(session.set_state(SessionState::Setup).is_err());
}

#[test]
fn test_volume_control() {
    let mut session = ReceiverSession::new(test_addr());

    session.set_volume(-10.0);
    assert!((session.volume() - -10.0).abs() < f32::EPSILON);

    // Test clamping
    session.set_volume(-200.0);
    assert!((session.volume() - -144.0).abs() < f32::EPSILON);

    session.set_volume(10.0);
    assert!((session.volume() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_stream_params() {
    let mut session = ReceiverSession::new(test_addr());
    assert!(session.stream_params().is_none());

    let params = StreamParameters::default();
    session.set_stream_params(params.clone());

    assert!(session.stream_params().is_some());
    assert_eq!(session.stream_params().unwrap().sample_rate, 44100);
}

#[test]
fn test_sockets() {
    let mut session = ReceiverSession::new(test_addr());
    assert!(session.sockets().is_none());

    let sockets = SessionSockets {
        audio_port: 1000,
        control_port: 1001,
        timing_port: 1002,
        client_control_port: Some(2001),
        client_timing_port: Some(2002),
        client_addr: None,
    };
    session.set_sockets(sockets);

    assert!(session.sockets().is_some());
    assert_eq!(session.sockets().unwrap().audio_port, 1000);
}

#[test]
fn test_rtsp_session_id() {
    let mut session = ReceiverSession::new(test_addr());
    assert!(session.rtsp_session_id().is_none());

    session.set_rtsp_session_id("TEST_ID".to_string());
    assert_eq!(session.rtsp_session_id(), Some("TEST_ID"));
}

#[test]
fn test_state_properties() {
    let state = SessionState::Streaming;
    assert!(state.is_active());
    assert!(state.is_valid());

    let state = SessionState::Paused;
    assert!(state.is_active());
    assert!(state.is_valid());

    let state = SessionState::Connected;
    assert!(!state.is_active());
    assert!(state.is_valid());

    let state = SessionState::Teardown;
    assert!(!state.is_active());
    assert!(!state.is_valid());
}
