use crate::protocol::crypto::Ed25519KeyPair;
use crate::receiver::ap2::config::Ap2Config;
use crate::receiver::ap2::password_auth::PasswordAuthManager;

#[test]
fn test_password_manager_lifecycle() {
    let identity = Ed25519KeyPair::generate();
    let mut manager = PasswordAuthManager::new(identity);

    // Initially disabled
    assert!(!manager.is_enabled());

    // Enable with password
    manager.set_password("secret123".to_string());
    assert!(manager.is_enabled());

    // Disable
    manager.clear_password();
    assert!(!manager.is_enabled());
}

#[test]
fn test_password_validation_rules() {
    // Too short
    assert!(Ap2Config::validate_password("abc").is_err());

    // Empty
    assert!(Ap2Config::validate_password("").is_err());

    // Valid minimum length
    assert!(Ap2Config::validate_password("abcd").is_ok());

    // Valid longer password
    assert!(Ap2Config::validate_password("my_secure_password_123").is_ok());
}

#[test]
fn test_lockout_disabled_by_default() {
    let identity = Ed25519KeyPair::generate();
    let manager = PasswordAuthManager::new(identity);

    assert!(!manager.is_locked_out());
    assert!(manager.lockout_remaining().is_none());
}
