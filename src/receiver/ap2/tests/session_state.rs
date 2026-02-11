use crate::receiver::ap2::session_state::Ap2SessionState;

#[test]
fn test_valid_pairing_flow() {
    let mut state = Ap2SessionState::Connected;

    state = state.transition_to(Ap2SessionState::InfoExchanged).unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 1 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 2 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 3 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 4 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 1 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 2 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 3 })
        .unwrap();
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 4 })
        .unwrap();
    state = state.transition_to(Ap2SessionState::Paired).unwrap();

    assert!(state.is_authenticated());
    assert!(state.requires_encryption());
}

#[test]
fn test_invalid_transition() {
    let state = Ap2SessionState::Connected;

    // Cannot go directly to Streaming
    let result = state.transition_to(Ap2SessionState::Streaming);
    assert!(result.is_err());
}

#[test]
fn test_method_permissions() {
    let state = Ap2SessionState::Connected;
    assert!(state.allows_method("OPTIONS"));
    assert!(state.allows_method("GET"));
    assert!(!state.allows_method("SETUP"));

    let state = Ap2SessionState::Paired;
    assert!(state.allows_method("SETUP"));
    assert!(!state.allows_method("RECORD"));

    let state = Ap2SessionState::SetupPhase2;
    assert!(state.allows_method("RECORD"));
}

#[test]
fn test_valid_streaming_flow() {
    let mut state = Ap2SessionState::Paired;

    state = state.transition_to(Ap2SessionState::SetupPhase1).unwrap();
    state = state.transition_to(Ap2SessionState::SetupPhase2).unwrap();
    state = state.transition_to(Ap2SessionState::Streaming).unwrap();

    assert!(state.is_streaming());
    assert!(state.is_authenticated());

    state = state.transition_to(Ap2SessionState::Paused).unwrap();
    assert!(!state.is_streaming());

    state = state.transition_to(Ap2SessionState::Streaming).unwrap();
    assert!(state.is_streaming());

    state = state.transition_to(Ap2SessionState::Teardown).unwrap();
    assert!(!state.is_streaming());
}

#[test]
fn test_invalid_setup_flow() {
    let state = Ap2SessionState::Paired;
    // Cannot skip SetupPhase1
    assert!(state.transition_to(Ap2SessionState::SetupPhase2).is_err());

    let state = Ap2SessionState::SetupPhase1;
    // Cannot skip SetupPhase2
    assert!(state.transition_to(Ap2SessionState::Streaming).is_err());
}

#[test]
fn test_error_state_handling() {
    let state = Ap2SessionState::Connected;
    let error_state = Ap2SessionState::Error {
        code: 500,
        message: "Internal Error".to_string(),
    };

    // Can transition to Error from anywhere
    assert!(state.transition_to(error_state.clone()).is_ok());

    // Error -> Teardown is INVALID because of the guard
    assert!(
        error_state
            .transition_to(Ap2SessionState::Teardown)
            .is_err()
    );
}

#[test]
fn test_teardown_constraints() {
    // Connected -> Teardown is NOT valid (connection just closes)
    assert!(
        Ap2SessionState::Connected
            .transition_to(Ap2SessionState::Teardown)
            .is_err()
    );

    // Paired -> Teardown IS valid
    assert!(
        Ap2SessionState::Paired
            .transition_to(Ap2SessionState::Teardown)
            .is_ok()
    );
}
