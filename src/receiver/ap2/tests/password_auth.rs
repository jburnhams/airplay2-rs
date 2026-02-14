use crate::protocol::crypto::Ed25519KeyPair;
use crate::protocol::pairing::tlv::{TlvEncoder, TlvType};
use crate::receiver::ap2::config::Ap2Config;
use crate::receiver::ap2::password_auth::{PasswordAuthManager, PasswordAuthError};

#[test]
fn test_password_validation() {
    // Valid passwords
    assert!(Ap2Config::validate_password("1234").is_ok());
    assert!(Ap2Config::validate_password("password123").is_ok());

    // Invalid passwords
    assert!(Ap2Config::validate_password("").is_err());
    assert!(Ap2Config::validate_password("123").is_err()); // Too short
}

#[test]
fn test_manager_creation() {
    let identity = Ed25519KeyPair::generate();
    let manager = PasswordAuthManager::new(identity);

    assert!(!manager.is_enabled());
    assert!(!manager.is_locked_out());
}

#[test]
fn test_set_password_enables_auth() {
    let identity = Ed25519KeyPair::generate();
    let mut manager = PasswordAuthManager::new(identity);

    manager.set_password("test1234".to_string());
    assert!(manager.is_enabled());

    manager.clear_password();
    assert!(!manager.is_enabled());
}

#[test]
fn test_lockout_logic() {
    let identity = Ed25519KeyPair::generate();
    let mut manager = PasswordAuthManager::new(identity);
    manager.set_password("correct".to_string());

    // Simulate failed attempts
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    // 1. M1
    let res = manager.process_pair_setup(&m1).unwrap();
    assert!(res.error.is_none());

    // 2. M3 with bad proof
    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, &[0u8; 32]) // Dummy key
        .add(TlvType::Proof, &[0u8; 32])      // Dummy proof
        .build();

    // This should fail
    let res = manager.process_pair_setup(&m3).unwrap();
    assert!(res.error.is_some());

    // Do this 5 times to trigger lockout
    for _ in 0..4 {
        let _ = manager.process_pair_setup(&m1); // Start over
        let res = manager.process_pair_setup(&m3); // Fail
        assert!(res.unwrap().error.is_some());
    }

    // Now should be locked out
    let res = manager.process_pair_setup(&m1);
    match res {
        Err(PasswordAuthError::LockedOut { .. }) => (),
        _ => panic!("Should be locked out"),
    }
}
