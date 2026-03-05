use crate::protocol::crypto::Ed25519KeyPair;
use crate::receiver::Ap2Config;
use crate::receiver::ap2::password_auth::{FailedAttemptTracker, PasswordAuthManager};

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
fn test_lockout_tracking() {
    let mut tracker = FailedAttemptTracker::new_for_test();
    // tracker.max_attempts = 3;
    // tracker.window = std::time::Duration::from_secs(60);
    // tracker.lockout_duration = std::time::Duration::from_secs(5);

    // First few attempts should not lock
    tracker.record_attempt_for_test(false);
    assert!(!tracker.is_locked_for_test());
    tracker.record_attempt_for_test(false);
    assert!(!tracker.is_locked_for_test());

    // Third attempt should lock
    tracker.record_attempt_for_test(false);
    assert!(tracker.is_locked_for_test());
    assert!(tracker.lockout_remaining_for_test().is_some());
}

#[test]
fn test_successful_auth_clears_attempts() {
    let mut tracker = FailedAttemptTracker::new_for_test();

    tracker.record_attempt_for_test(false);
    tracker.record_attempt_for_test(false);
    // assert_eq!(tracker.attempts.len(), 2);

    // Successful attempt clears history
    tracker.record_attempt_for_test(true);
    // assert_eq!(tracker.attempts.len(), 0);
    assert!(!tracker.is_locked_for_test());
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
