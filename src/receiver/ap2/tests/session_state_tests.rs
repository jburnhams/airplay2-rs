use crate::receiver::ap2::session_state::Ap2SessionState;

#[test]
fn test_valid_pairing_flow() {
    let mut state = Ap2SessionState::Connected;

    state = state.transition_to(Ap2SessionState::InfoExchanged).unwrap();
    state = state.transition_to(Ap2SessionState::PairingSetup { step: 1 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingSetup { step: 2 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingSetup { step: 3 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingSetup { step: 4 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingVerify { step: 1 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingVerify { step: 2 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingVerify { step: 3 }).unwrap();
    state = state.transition_to(Ap2SessionState::PairingVerify { step: 4 }).unwrap();
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
