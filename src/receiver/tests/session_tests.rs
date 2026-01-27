use super::super::session::{ReceiverSession, SessionState};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

fn test_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 12345)
}

#[test]
fn test_valid_state_transitions() {
    use SessionState::{Announced, Connected, Paused, Setup, Streaming, Teardown};

    assert!(Connected.can_transition_to(Announced));
    assert!(Announced.can_transition_to(Setup));
    assert!(Setup.can_transition_to(Streaming));
    assert!(Streaming.can_transition_to(Paused));
    assert!(Paused.can_transition_to(Streaming));
    assert!(Streaming.can_transition_to(Teardown));
}

#[test]
fn test_invalid_state_transitions() {
    use SessionState::{Closed, Connected, Setup, Streaming};

    assert!(!Connected.can_transition_to(Streaming));
    assert!(!Setup.can_transition_to(Connected));
    assert!(!Closed.can_transition_to(Connected));
}

#[test]
fn test_teardown_from_any_state() {
    use SessionState::{Announced, Connected, Paused, Setup, Streaming, Teardown};

    for state in [Connected, Announced, Setup, Streaming, Paused] {
        assert!(
            state.can_transition_to(Teardown),
            "{state:?} should transition to Teardown"
        );
    }
}

#[test]
fn test_session_state_change() {
    let mut session = ReceiverSession::new(test_addr());

    assert_eq!(session.state(), SessionState::Connected);

    session.set_state(SessionState::Announced).unwrap();
    assert_eq!(session.state(), SessionState::Announced);

    // Invalid transition should fail
    let result = session.set_state(SessionState::Streaming);
    assert!(result.is_err());
}

#[test]
fn test_session_volume() {
    let mut session = ReceiverSession::new(test_addr());

    assert_eq!(session.volume(), 0.0);

    session.set_volume(-15.0);
    assert_eq!(session.volume(), -15.0);

    // Clamp to valid range
    session.set_volume(-200.0);
    assert_eq!(session.volume(), -144.0);

    session.set_volume(10.0);
    assert_eq!(session.volume(), 0.0);
}

#[test]
fn test_session_timeout() {
    let session = ReceiverSession::new(test_addr());

    // Immediately after creation, should not be timed out
    assert!(!session.is_timed_out(Duration::from_secs(1)));

    // With zero timeout, should be timed out
    std::thread::sleep(Duration::from_millis(1));
    assert!(session.is_timed_out(Duration::ZERO));
}

#[test]
fn test_is_active_states() {
    assert!(SessionState::Streaming.is_active());
    assert!(SessionState::Paused.is_active());
    assert!(!SessionState::Connected.is_active());
    assert!(!SessionState::Teardown.is_active());
}
