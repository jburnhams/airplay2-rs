use std::io;
use thiserror::Error;

/// RAOP-specific errors
#[derive(Debug, Error)]
pub enum RaopError {
    /// RSA authentication failed
    #[error("RSA authentication failed")]
    AuthenticationFailed,

    /// Unsupported encryption type
    #[error("unsupported encryption type: {0}")]
    UnsupportedEncryption(String),

    /// SDP parsing error
    #[error("SDP parsing error: {0}")]
    SdpParseError(String),

    /// Key exchange failed
    #[error("key exchange failed: {0}")]
    KeyExchangeFailed(String),

    /// Audio encryption error
    #[error("audio encryption error: {0}")]
    EncryptionError(String),

    /// Timing synchronization failed
    #[error("timing sync failed")]
    TimingSyncFailed,

    /// Retransmit buffer overflow
    #[error("retransmit buffer overflow")]
    RetransmitBufferOverflow,
}

/// Errors that can occur during `AirPlay` operations
#[derive(Debug, Error)]
pub enum AirPlayError {
    /// RAOP error
    #[error("RAOP error: {0}")]
    Raop(#[from] RaopError),

    // ===== Discovery Errors =====
    /// Device was not found during discovery
    #[error("device not found: {device_id}")]
    DeviceNotFound {
        /// The ID of the device that was not found
        device_id: String,
    },

    /// mDNS discovery failed
    #[error("discovery failed: {message}")]
    DiscoveryFailed {
        /// Description of the failure
        message: String,
        /// The underlying source of the error
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    // ===== Connection Errors =====
    /// Failed to establish connection to device
    #[error("connection failed to {device_name}: {message}")]
    ConnectionFailed {
        /// The name of the device
        device_name: String,
        /// Description of the failure
        message: String,
        /// The underlying source of the error
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Connection was closed unexpectedly
    #[error("device disconnected: {device_name}")]
    Disconnected {
        /// The name of the device
        device_name: String,
    },

    /// Connection timed out
    #[error("connection timeout after {duration:?}")]
    ConnectionTimeout {
        /// The duration of the timeout
        duration: std::time::Duration,
    },

    // ===== Authentication Errors =====
    /// Pairing/authentication failed
    #[error("authentication failed: {message}")]
    AuthenticationFailed {
        /// Description of the failure
        message: String,
        /// Whether the error is recoverable by retrying
        recoverable: bool,
    },

    /// Pairing required but not initiated
    #[error("pairing required with device {device_name}")]
    PairingRequired {
        /// The name of the device
        device_name: String,
    },

    /// Stored pairing keys are invalid or expired
    #[error("pairing keys invalid for device {device_id}")]
    PairingInvalid {
        /// The ID of the device
        device_id: String,
    },

    // ===== Protocol Errors =====
    /// RTSP protocol error
    #[error("RTSP error: {message}")]
    RtspError {
        /// Description of the error
        message: String,
        /// HTTP/RTSP status code if available
        status_code: Option<u16>,
    },

    /// RTP protocol error
    #[error("RTP error: {message}")]
    RtpError {
        /// Description of the error
        message: String,
    },

    /// Unexpected protocol response
    #[error("unexpected response: expected {expected}, got {actual}")]
    UnexpectedResponse {
        /// What was expected
        expected: String,
        /// What was actually received
        actual: String,
    },

    /// Protocol message encoding/decoding failed
    #[error("codec error: {message}")]
    CodecError {
        /// Description of the error
        message: String,
    },

    // ===== Playback Errors =====
    /// Playback error from device
    #[error("playback error: {message}")]
    PlaybackError {
        /// Description of the error
        message: String,
    },

    /// Invalid URL for streaming
    #[error("invalid URL: {url} - {reason}")]
    InvalidUrl {
        /// The invalid URL
        url: String,
        /// Reason why it is invalid
        reason: String,
    },

    /// Audio format not supported
    #[error("unsupported audio format: {format}")]
    UnsupportedFormat {
        /// The unsupported format
        format: String,
    },

    /// Queue operation failed
    #[error("queue error: {message}")]
    QueueError {
        /// Description of the error
        message: String,
    },

    /// Seek position out of range
    #[error("seek position {position} out of range (duration: {duration:?})")]
    SeekOutOfRange {
        /// The requested position
        position: f64,
        /// The duration of the track
        duration: Option<f64>,
    },

    // ===== I/O Errors =====
    /// Network I/O error
    #[error("network error: {0}")]
    NetworkError(#[from] io::Error),

    /// Operation timed out
    #[error("operation timed out")]
    Timeout,

    // ===== State Errors =====
    /// Operation not valid in current state
    #[error("invalid state: {message}")]
    InvalidState {
        /// Description of why the state is invalid
        message: String,
        /// The current state
        current_state: String,
    },

    /// Device is busy with another operation
    #[error("device busy")]
    DeviceBusy,

    // ===== Internal Errors =====
    /// Internal library error
    #[error("internal error: {message}")]
    InternalError {
        /// Description of the error
        message: String,
    },

    /// Feature not yet implemented
    #[error("not implemented: {feature}")]
    NotImplemented {
        /// The feature that is not implemented
        feature: String,
    },

    /// Invalid parameter provided
    #[error("invalid parameter: {name} - {message}")]
    InvalidParameter {
        /// The name of the parameter
        name: String,
        /// Description of the error
        message: String,
    },

    /// General I/O error
    #[error("I/O error: {message}")]
    IoError {
        /// Description of the error
        message: String,
        /// The underlying source of the error
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl AirPlayError {
    /// Check if this error is recoverable by retrying
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionTimeout { .. }
                | Self::Timeout
                | Self::NetworkError(_)
                | Self::DeviceBusy
        )
    }

    /// Check if this error indicates connection loss
    #[must_use]
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            Self::Disconnected { .. }
                | Self::ConnectionFailed { .. }
                | Self::ConnectionTimeout { .. }
        )
    }
}

/// Result type alias for `AirPlay` operations
pub type Result<T> = std::result::Result<T, AirPlayError>;

#[cfg(test)]
mod tests {
    use super::*;

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
}
