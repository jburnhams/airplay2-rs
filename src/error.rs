//! Error types

/// Main error type for `AirPlay` operations.
#[derive(Debug, thiserror::Error)]
pub enum AirPlayError {
    /// Connection to the device failed.
    #[error("connection failed")]
    ConnectionFailed,
    // Add other variants
}
