use std::io;

use crate::error::*;

#[test]
fn test_raop_error_display() {
    assert_eq!(
        RaopError::AuthenticationFailed.to_string(),
        "RSA authentication failed"
    );

    assert_eq!(
        RaopError::UnsupportedEncryption("aes-256".to_string()).to_string(),
        "unsupported encryption type: aes-256"
    );

    assert_eq!(
        RaopError::SdpParseError("missing audio info".to_string()).to_string(),
        "SDP parsing error: missing audio info"
    );

    assert_eq!(
        RaopError::KeyExchangeFailed("bad signature".to_string()).to_string(),
        "key exchange failed: bad signature"
    );

    assert_eq!(
        RaopError::EncryptionError("nonce too short".to_string()).to_string(),
        "audio encryption error: nonce too short"
    );

    assert_eq!(
        RaopError::TimingSyncFailed.to_string(),
        "timing sync failed"
    );

    assert_eq!(
        RaopError::RetransmitBufferOverflow.to_string(),
        "retransmit buffer overflow"
    );
}

#[test]
fn test_airplay_error_display_raop() {
    let err = AirPlayError::Raop(RaopError::TimingSyncFailed);
    assert_eq!(err.to_string(), "RAOP error: timing sync failed");
}

#[test]
fn test_airplay_error_display_discovery() {
    let err = AirPlayError::DeviceNotFound {
        device_id: "ABC123".to_string(),
    };
    assert_eq!(err.to_string(), "device not found: ABC123");

    let err = AirPlayError::DiscoveryFailed {
        message: "mdns timeout".to_string(),
        source: None,
    };
    assert_eq!(err.to_string(), "discovery failed: mdns timeout");
}

#[test]
fn test_airplay_error_display_connection() {
    let err = AirPlayError::ConnectionFailed {
        device_name: "Living Room".to_string(),
        message: "connection refused".to_string(),
        source: None,
    };
    assert_eq!(
        err.to_string(),
        "connection failed to Living Room: connection refused"
    );

    let err = AirPlayError::Disconnected {
        device_name: "Living Room".to_string(),
    };
    assert_eq!(err.to_string(), "device disconnected: Living Room");

    let err = AirPlayError::ConnectionTimeout {
        duration: std::time::Duration::from_secs(5),
    };
    assert_eq!(err.to_string(), "connection timeout after 5s");
}

#[test]
fn test_airplay_error_display_authentication() {
    let err = AirPlayError::AuthenticationFailed {
        message: "invalid PIN".to_string(),
        recoverable: true,
    };
    assert_eq!(err.to_string(), "authentication failed: invalid PIN");

    let err = AirPlayError::PairingRequired {
        device_name: "Kitchen".to_string(),
    };
    assert_eq!(err.to_string(), "pairing required with device Kitchen");

    let err = AirPlayError::PairingInvalid {
        device_id: "ID-123".to_string(),
    };
    assert_eq!(err.to_string(), "pairing keys invalid for device ID-123");
}

#[test]
fn test_airplay_error_display_protocol() {
    let err = AirPlayError::RtspError {
        message: "bad request".to_string(),
        status_code: Some(400),
    };
    assert_eq!(err.to_string(), "RTSP error: bad request");

    let err = AirPlayError::RtpError {
        message: "sequence mismatch".to_string(),
    };
    assert_eq!(err.to_string(), "RTP error: sequence mismatch");

    let err = AirPlayError::UnexpectedResponse {
        expected: "200 OK".to_string(),
        actual: "404 Not Found".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "unexpected response: expected 200 OK, got 404 Not Found"
    );

    let err = AirPlayError::CodecError {
        message: "missing plist header".to_string(),
    };
    assert_eq!(err.to_string(), "codec error: missing plist header");
}

#[test]
fn test_airplay_error_display_playback() {
    let err = AirPlayError::PlaybackError {
        message: "buffer underflow".to_string(),
    };
    assert_eq!(err.to_string(), "playback error: buffer underflow");

    let err = AirPlayError::InvalidUrl {
        url: "http://invalid".to_string(),
        reason: "unresolvable host".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "invalid URL: http://invalid - unresolvable host"
    );

    let err = AirPlayError::UnsupportedFormat {
        format: "ogg".to_string(),
    };
    assert_eq!(err.to_string(), "unsupported audio format: ogg");

    let err = AirPlayError::QueueError {
        message: "queue full".to_string(),
    };
    assert_eq!(err.to_string(), "queue error: queue full");

    let err = AirPlayError::SeekOutOfRange {
        position: 120.0,
        duration: Some(100.0),
    };
    assert_eq!(
        err.to_string(),
        "seek position 120 out of range (duration: Some(100.0))"
    );
}

#[test]
fn test_airplay_error_display_io_state_internal() {
    let err = AirPlayError::NetworkError(io::Error::other("socket closed"));
    assert_eq!(err.to_string(), "network error: socket closed");

    assert_eq!(AirPlayError::Timeout.to_string(), "operation timed out");

    let err = AirPlayError::InvalidState {
        message: "cannot play while connecting".to_string(),
        current_state: "connecting".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "invalid state: cannot play while connecting"
    );

    assert_eq!(AirPlayError::DeviceBusy.to_string(), "device busy");

    let err = AirPlayError::InternalError {
        message: "null pointer".to_string(),
    };
    assert_eq!(err.to_string(), "internal error: null pointer");

    let err = AirPlayError::NotImplemented {
        feature: "FairPlay".to_string(),
    };
    assert_eq!(err.to_string(), "not implemented: FairPlay");

    let err = AirPlayError::InvalidParameter {
        name: "volume".to_string(),
        message: "out of bounds".to_string(),
    };
    assert_eq!(err.to_string(), "invalid parameter: volume - out of bounds");

    let err = AirPlayError::IoError {
        message: "disk full".to_string(),
        source: None,
    };
    assert_eq!(err.to_string(), "I/O error: disk full");
}

#[test]
fn test_error_is_recoverable() {
    // Recoverable errors
    assert!(AirPlayError::Timeout.is_recoverable());
    assert!(AirPlayError::DeviceBusy.is_recoverable());
    assert!(
        AirPlayError::ConnectionTimeout {
            duration: std::time::Duration::from_secs(1)
        }
        .is_recoverable()
    );
    assert!(
        AirPlayError::NetworkError(io::Error::new(io::ErrorKind::ConnectionReset, "reset"))
            .is_recoverable()
    );

    // AuthenticationFailed respects its recoverable field
    let auth_err_recoverable = AirPlayError::AuthenticationFailed {
        message: "wait".to_string(),
        recoverable: true,
    };
    assert!(auth_err_recoverable.is_recoverable());

    let auth_err_fatal = AirPlayError::AuthenticationFailed {
        message: "bad pin".to_string(),
        recoverable: false,
    };
    assert!(!auth_err_fatal.is_recoverable());

    // Non-recoverable errors
    assert!(
        !AirPlayError::Disconnected {
            device_name: "test".to_string()
        }
        .is_recoverable()
    );
}

#[test]
fn test_error_is_connection_lost() {
    // Connection lost errors
    assert!(
        AirPlayError::Disconnected {
            device_name: "HomePod".to_string(),
        }
        .is_connection_lost()
    );
    assert!(
        AirPlayError::ConnectionFailed {
            device_name: "Apple TV".to_string(),
            message: "refused".to_string(),
            source: None,
        }
        .is_connection_lost()
    );
    assert!(
        AirPlayError::ConnectionTimeout {
            duration: std::time::Duration::from_secs(1)
        }
        .is_connection_lost()
    );

    // Non connection lost errors
    assert!(!AirPlayError::Timeout.is_connection_lost());
    assert!(!AirPlayError::DeviceBusy.is_connection_lost());
}

#[test]
fn test_error_from_io() {
    let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
    let err: AirPlayError = io_err.into();

    assert!(matches!(err, AirPlayError::NetworkError(_)));
}

#[test]
fn test_error_from_raop() {
    let raop_err = RaopError::TimingSyncFailed;
    let err: AirPlayError = raop_err.into();

    assert!(matches!(
        err,
        AirPlayError::Raop(RaopError::TimingSyncFailed)
    ));
}

#[test]
fn test_error_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RaopError>();
    assert_send_sync::<AirPlayError>();
}
