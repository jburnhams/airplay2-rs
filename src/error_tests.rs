use crate::error::*;
use std::io;

#[test]
fn test_error_display() {
    let err = AirPlayError::DeviceNotFound {
        device_id: "ABC123".to_string(),
    };
    assert_eq!(err.to_string(), "device not found: ABC123");
}

#[test]
fn test_error_is_recoverable() {
    assert!(AirPlayError::Timeout.is_recoverable());
    assert!(AirPlayError::DeviceBusy.is_recoverable());

    let auth_err = AirPlayError::AuthenticationFailed {
        message: "bad pin".to_string(),
        recoverable: false,
    };
    assert!(!auth_err.is_recoverable());
}

#[test]
fn test_error_is_connection_lost() {
    let err = AirPlayError::Disconnected {
        device_name: "HomePod".to_string(),
    };
    assert!(err.is_connection_lost());
    assert!(!AirPlayError::Timeout.is_connection_lost());
}

#[test]
fn test_error_from_io() {
    let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
    let err: AirPlayError = io_err.into();

    assert!(matches!(err, AirPlayError::NetworkError(_)));
}

#[test]
fn test_error_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AirPlayError>();
}
