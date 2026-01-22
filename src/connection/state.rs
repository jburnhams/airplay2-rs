//! Connection state management

use crate::types::AirPlayDevice;
use std::time::Instant;

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// TCP connection in progress
    Connecting,
    /// Pairing/authentication in progress
    Authenticating,
    /// Setting up RTSP session
    SettingUp,
    /// Fully connected and ready
    Connected,
    /// Connection lost, attempting reconnect
    Reconnecting,
    /// Fatal error, cannot reconnect
    Failed,
}

impl ConnectionState {
    /// Check if currently connected or connecting
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting
                | ConnectionState::Authenticating
                | ConnectionState::SettingUp
                | ConnectionState::Connected
                | ConnectionState::Reconnecting
        )
    }

    /// Check if fully connected
    #[must_use]
    pub fn is_connected(self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Check if in a failed state
    #[must_use]
    pub fn is_failed(self) -> bool {
        matches!(self, ConnectionState::Failed | ConnectionState::Disconnected)
    }
}

/// Connection events
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// State changed
    StateChanged {
        /// The previous state
        old: ConnectionState,
        /// The new state
        new: ConnectionState,
    },
    /// Connection established
    Connected {
        /// The connected device
        device: AirPlayDevice,
    },
    /// Connection lost
    Disconnected {
        /// The disconnected device
        device: AirPlayDevice,
        /// The reason for disconnection
        reason: DisconnectReason,
    },
    /// Pairing required (need PIN)
    PairingRequired {
        /// The device requiring pairing
        device: AirPlayDevice,
    },
    /// Error occurred
    Error {
        /// The error message
        message: String,
        /// Whether the error is recoverable
        recoverable: bool,
    },
}

/// Reason for disconnection
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// User requested disconnect
    UserRequested,
    /// Network error
    NetworkError(String),
    /// Device went offline
    DeviceOffline,
    /// Authentication failed
    AuthenticationFailed,
    /// Protocol error
    ProtocolError(String),
    /// Timeout
    Timeout,
}

/// Connection statistics
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    /// Time connection was established
    pub connected_at: Option<Instant>,
    /// Number of bytes sent
    pub bytes_sent: u64,
    /// Number of bytes received
    pub bytes_received: u64,
    /// Number of reconnection attempts
    pub reconnect_attempts: u32,
    /// Last error message
    pub last_error: Option<String>,
    /// Round-trip time (if measured)
    pub rtt_ms: Option<u32>,
}

impl ConnectionStats {
    /// Get connection uptime
    #[must_use]
    pub fn uptime(&self) -> Option<std::time::Duration> {
        self.connected_at.map(|t| t.elapsed())
    }

    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: usize) {
        self.bytes_sent += bytes as u64;
    }

    /// Record bytes received
    pub fn record_received(&mut self, bytes: usize) {
        self.bytes_received += bytes as u64;
    }
}
