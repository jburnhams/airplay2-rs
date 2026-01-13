# Section 10: Connection Management

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 05**: RTSP Protocol (must be complete)
- **Section 07**: HomeKit Pairing (must be complete)
- **Section 09**: Async Runtime Abstraction (must be complete)

## Overview

This section manages the lifecycle of connections to AirPlay devices, including:
- TCP connection establishment
- Pairing/authentication
- RTSP session management
- Encrypted channel maintenance
- Reconnection logic

## Objectives

- Implement connection state machine
- Handle pairing negotiation
- Manage encrypted RTSP sessions
- Support automatic reconnection
- Provide connection events/callbacks

---

## Tasks

### 10.1 Connection State

- [ ] **10.1.1** Define connection state and events

**File:** `src/connection/state.rs`

```rust
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
    pub fn is_active(&self) -> bool {
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
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Check if in a failed state
    pub fn is_failed(&self) -> bool {
        matches!(self, ConnectionState::Failed | ConnectionState::Disconnected)
    }
}

/// Connection events
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// State changed
    StateChanged {
        old: ConnectionState,
        new: ConnectionState,
    },
    /// Connection established
    Connected {
        device: AirPlayDevice,
    },
    /// Connection lost
    Disconnected {
        device: AirPlayDevice,
        reason: DisconnectReason,
    },
    /// Pairing required (need PIN)
    PairingRequired {
        device: AirPlayDevice,
    },
    /// Error occurred
    Error {
        message: String,
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
```

---

### 10.2 Connection Manager

- [ ] **10.2.1** Implement the connection manager

**File:** `src/connection/manager.rs`

```rust
//! Connection manager for AirPlay devices

use super::state::{ConnectionState, ConnectionEvent, ConnectionStats, DisconnectReason};
use crate::types::{AirPlayDevice, AirPlayConfig};
use crate::error::AirPlayError;
use crate::protocol::rtsp::{RtspSession, RtspCodec, RtspRequest, RtspResponse, Method};
use crate::protocol::pairing::{TransientPairing, PairVerify, SessionKeys, PairingStorage, PairingKeys};
use crate::net::{TcpStream, AsyncReadExt, AsyncWriteExt, Runtime};

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::net::UdpSocket;

/// Connection manager handles device connections
pub struct ConnectionManager {
    /// Configuration
    config: AirPlayConfig,
    /// Current state
    state: RwLock<ConnectionState>,
    /// Connected device info
    device: RwLock<Option<AirPlayDevice>>,
    /// TCP connection
    stream: Mutex<Option<TcpStream>>,
    /// UDP sockets (audio, control, timing)
    sockets: Mutex<Option<UdpSockets>>,
    /// RTSP session
    rtsp_session: Mutex<Option<RtspSession>>,
    /// RTSP codec
    rtsp_codec: Mutex<RtspCodec>,
    /// Session keys (after pairing)
    session_keys: Mutex<Option<SessionKeys>>,
    /// Connection statistics
    stats: RwLock<ConnectionStats>,
    /// Event sender
    event_tx: mpsc::UnboundedSender<ConnectionEvent>,
    /// Event receiver (clone for subscribers)
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<ConnectionEvent>>>,
    /// Pairing storage
    pairing_storage: Option<Box<dyn PairingStorage>>,
}

/// UDP sockets for streaming
struct UdpSockets {
    audio: UdpSocket,
    control: UdpSocket,
    timing: UdpSocket,
    server_audio_port: u16,
    server_control_port: u16,
    server_timing_port: u16,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(config: AirPlayConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            config,
            state: RwLock::new(ConnectionState::Disconnected),
            device: RwLock::new(None),
            stream: Mutex::new(None),
            sockets: Mutex::new(None),
            rtsp_session: Mutex::new(None),
            rtsp_codec: Mutex::new(RtspCodec::new()),
            session_keys: Mutex::new(None),
            stats: RwLock::new(ConnectionStats::default()),
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            pairing_storage: None,
        }
    }

    /// Set pairing storage for persistent pairing
    pub fn with_pairing_storage(mut self, storage: Box<dyn PairingStorage>) -> Self {
        self.pairing_storage = Some(storage);
        self
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Get connected device
    pub async fn device(&self) -> Option<AirPlayDevice> {
        self.device.read().await.clone()
    }

    /// Get connection statistics
    pub async fn stats(&self) -> ConnectionStats {
        self.stats.read().await.clone()
    }

    /// Connect to a device
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // Check if already connected
        let current_state = *self.state.read().await;
        if current_state.is_active() {
            return Err(AirPlayError::InvalidState {
                message: "Already connected or connecting".to_string(),
                current_state: format!("{:?}", current_state),
            });
        }

        self.set_state(ConnectionState::Connecting).await;
        *self.device.write().await = Some(device.clone());

        // Attempt connection with timeout
        let result = Runtime::timeout(
            self.config.connection_timeout,
            self.connect_internal(device),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                self.set_state(ConnectionState::Connected).await;
                self.send_event(ConnectionEvent::Connected {
                    device: device.clone(),
                });
                Ok(())
            }
            Ok(Err(e)) => {
                self.set_state(ConnectionState::Failed).await;
                self.send_event(ConnectionEvent::Error {
                    message: e.to_string(),
                    recoverable: e.is_recoverable(),
                });
                Err(e)
            }
            Err(_) => {
                self.set_state(ConnectionState::Failed).await;
                Err(AirPlayError::ConnectionTimeout {
                    duration: self.config.connection_timeout,
                })
            }
        }
    }

    /// Internal connection logic
    async fn connect_internal(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // 1. Establish TCP connection
        let addr = format!("{}:{}", device.address, device.port);
        tracing::debug!("Connecting to {}", addr);

        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            AirPlayError::ConnectionFailed {
                device_name: device.name.clone(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
        })?;

        *self.stream.lock().await = Some(stream);

        // 2. Initialize RTSP session
        let rtsp_session = RtspSession::new(&device.address.to_string(), device.port);
        *self.rtsp_session.lock().await = Some(rtsp_session);

        // 3. Perform OPTIONS exchange
        self.set_state(ConnectionState::SettingUp).await;
        self.send_options().await?;

        // 4. Authenticate if required
        self.set_state(ConnectionState::Authenticating).await;
        self.authenticate(device).await?;

        // 5. Setup RTSP session
        self.set_state(ConnectionState::SettingUp).await;
        self.setup_session().await?;

        Ok(())
    }

    /// Send RTSP OPTIONS and process response
    async fn send_options(&self) -> Result<(), AirPlayError> {
        let request = {
            let mut session = self.rtsp_session.lock().await;
            let session = session.as_mut().ok_or(AirPlayError::InvalidState {
                message: "No RTSP session".to_string(),
                current_state: "None".to_string(),
            })?;
            session.options_request()
        };

        let response = self.send_rtsp_request(&request).await?;

        {
            let mut session = self.rtsp_session.lock().await;
            let session = session.as_mut().unwrap();
            session
                .process_response(Method::Options, &response)
                .map_err(|e| AirPlayError::RtspError {
                    message: e,
                    status_code: Some(response.status.as_u16()),
                })?;
        }

        Ok(())
    }

    /// Authenticate with the device
    async fn authenticate(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // Check if we have stored keys
        if let Some(ref storage) = self.pairing_storage {
            if let Some(keys) = storage.load(&device.id) {
                // Try Pair-Verify with stored keys
                match self.pair_verify(device, &keys).await {
                    Ok(session_keys) => {
                        *self.session_keys.lock().await = Some(session_keys);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("Pair-Verify failed, trying transient: {}", e);
                    }
                }
            }
        }

        // Fall back to transient pairing
        let session_keys = self.transient_pair().await?;
        *self.session_keys.lock().await = Some(session_keys);

        Ok(())
    }

    /// Perform transient pairing
    async fn transient_pair(&self) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = TransientPairing::new().map_err(|e| {
            AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            }
        })?;

        // M1: Start pairing
        let m1 = pairing.start().map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        let m2 = self.send_pairing_data(&m1, "/pair-setup").await?;

        // M2 -> M3
        let result = pairing.process_m2(&m2).map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        let m3 = match result {
            crate::protocol::pairing::PairingStepResult::SendData(data) => data,
            crate::protocol::pairing::PairingStepResult::Complete(keys) => return Ok(keys),
            _ => {
                return Err(AirPlayError::AuthenticationFailed {
                    message: "Unexpected pairing state".to_string(),
                    recoverable: false,
                })
            }
        };

        let m4 = self.send_pairing_data(&m3, "/pair-setup").await?;

        // M4 -> Complete
        let result = pairing.process_m4(&m4).map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        match result {
            crate::protocol::pairing::PairingStepResult::Complete(keys) => Ok(keys),
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Pairing did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Perform Pair-Verify with stored keys
    async fn pair_verify(
        &self,
        device: &AirPlayDevice,
        keys: &PairingKeys,
    ) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = PairVerify::new(keys.clone(), &keys.device_public_key)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        // M1: Start verification
        let m1 = pairing.start().map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        let m2 = self.send_pairing_data(&m1, "/pair-verify").await?;

        // M2 -> M3
        let result = pairing.process_m2(&m2).map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        let m3 = match result {
            crate::protocol::pairing::PairingStepResult::SendData(data) => data,
            _ => return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state".to_string(),
                recoverable: false,
            }),
        };

        let m4 = self.send_pairing_data(&m3, "/pair-verify").await?;

        // M4 -> Complete
        let result = pairing.process_m4(&m4).map_err(|e| AirPlayError::AuthenticationFailed {
            message: e.to_string(),
            recoverable: false,
        })?;

        match result {
            crate::protocol::pairing::PairingStepResult::Complete(keys) => Ok(keys),
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Verification did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Setup RTSP session (SETUP command)
    async fn setup_session(&self) -> Result<(), AirPlayError> {
        // 1. Bind local UDP ports (0 = random port)
        let audio_sock = UdpSocket::bind("0.0.0.0:0").await?;
        let ctrl_sock = UdpSocket::bind("0.0.0.0:0").await?;
        let time_sock = UdpSocket::bind("0.0.0.0:0").await?;

        let audio_port = audio_sock.local_addr()?.port();
        let ctrl_port = ctrl_sock.local_addr()?.port();
        let time_port = time_sock.local_addr()?.port();

        // 2. Create SETUP request with transport parameters
        // Transport: RTP/AVP/UDP;unicast;mode=record;control_port=...;timing_port=...
        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={};timing_port={}",
            ctrl_port, time_port
        );

        let request = {
            let mut session = self.rtsp_session.lock().await;
            let session = session.as_mut().ok_or(AirPlayError::InvalidState {
                message: "No RTSP session".to_string(),
                current_state: "None".to_string(),
            })?;
            session.setup_request(&transport)
        };

        // 3. Send request
        let response = self.send_rtsp_request(&request).await?;

        // 4. Update session state
        {
            let mut session = self.rtsp_session.lock().await;
            let session = session.as_mut().unwrap();
            session.process_response(Method::Setup, &response).map_err(|e| AirPlayError::RtspError {
                message: e,
                status_code: Some(response.status.as_u16()),
            })?;
        }

        // 5. Parse response transport header to get server ports
        let transport_header = response.headers.get("Transport").ok_or(AirPlayError::RtspError {
            message: "Missing Transport header in SETUP response".to_string(),
            status_code: None,
        })?;

        // Parse server_port=X, control_port=Y, timing_port=Z
        // This is a simplified parser - real one would use regex or split
        let mut server_audio_port = 0;
        let mut server_ctrl_port = 0;
        let mut server_time_port = 0;

        for part in transport_header.split(';') {
            let kv: Vec<&str> = part.split('=').collect();
            if kv.len() == 2 {
                match kv[0].trim() {
                    "server_port" => server_audio_port = kv[1].parse().unwrap_or(0),
                    "control_port" => server_ctrl_port = kv[1].parse().unwrap_or(0),
                    "timing_port" => server_time_port = kv[1].parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if server_audio_port == 0 {
            return Err(AirPlayError::RtspError {
                message: "Could not determine server audio port".to_string(),
                status_code: None,
            });
        }

        // 6. Connect UDP sockets to server ports
        let device_ip = {
            let device = self.device.read().await;
            device.as_ref().unwrap().address
        };

        audio_sock.connect((device_ip, server_audio_port)).await?;
        ctrl_sock.connect((device_ip, server_ctrl_port)).await?;
        time_sock.connect((device_ip, server_time_port)).await?;

        *self.sockets.lock().await = Some(UdpSockets {
            audio: audio_sock,
            control: ctrl_sock,
            timing: time_sock,
            server_audio_port,
            server_control_port: server_ctrl_port,
            server_timing_port: server_time_port,
        });

        // 7. Send RECORD to start buffering
        let record_request = {
            let mut session = self.rtsp_session.lock().await;
            session.as_mut().unwrap().record_request()
        };
        self.send_rtsp_request(&record_request).await?;

        Ok(())
    }

    /// Send pairing data to device
    async fn send_pairing_data(&self, data: &[u8], path: &str) -> Result<Vec<u8>, AirPlayError> {
        // Send as HTTP POST
        let request = format!(
            "POST {} HTTP/1.1\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Length: {}\r\n\
             \r\n",
            path,
            data.len()
        );

        let mut stream = self.stream.lock().await;
        let stream = stream.as_mut().ok_or(AirPlayError::Disconnected {
            device_name: "unknown".to_string(),
        })?;

        // Send request
        stream.write_all(request.as_bytes()).await?;
        stream.write_all(data).await?;
        stream.flush().await?;

        // Read response
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await?;

        // Parse response and extract body
        // (simplified - should properly parse HTTP response)
        let response = &buf[..n];
        if let Some(body_start) = response.windows(4).position(|w| w == b"\r\n\r\n") {
            Ok(response[body_start + 4..].to_vec())
        } else {
            Err(AirPlayError::RtspError {
                message: "Invalid response".to_string(),
                status_code: None,
            })
        }
    }

    /// Send RTSP request and get response
    async fn send_rtsp_request(&self, request: &RtspRequest) -> Result<RtspResponse, AirPlayError> {
        let encoded = request.encode();

        let mut stream = self.stream.lock().await;
        let stream = stream.as_mut().ok_or(AirPlayError::Disconnected {
            device_name: "unknown".to_string(),
        })?;

        // Send request
        stream.write_all(&encoded).await?;
        stream.flush().await?;

        // Update stats
        self.stats.write().await.record_sent(encoded.len());

        // Read response
        let mut codec = self.rtsp_codec.lock().await;
        let mut buf = vec![0u8; 4096];

        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                return Err(AirPlayError::Disconnected {
                    device_name: "unknown".to_string(),
                });
            }

            self.stats.write().await.record_received(n);

            codec.feed(&buf[..n]).map_err(|e| AirPlayError::RtspError {
                message: e.to_string(),
                status_code: None,
            })?;

            if let Some(response) = codec.decode().map_err(|e| AirPlayError::RtspError {
                message: e.to_string(),
                status_code: None,
            })? {
                return Ok(response);
            }
        }
    }

    /// Disconnect from device
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        let device = self.device.read().await.clone();

        // Send TEARDOWN if connected
        if self.state().await == ConnectionState::Connected {
            let request = {
                let mut session = self.rtsp_session.lock().await;
                session.as_mut().map(|s| s.teardown_request())
            };

            if let Some(request) = request {
                let _ = self.send_rtsp_request(&request).await;
            }
        }

        // Close connection
        *self.stream.lock().await = None;
        *self.sockets.lock().await = None;
        *self.rtsp_session.lock().await = None;
        *self.session_keys.lock().await = None;

        self.set_state(ConnectionState::Disconnected).await;

        if let Some(device) = device {
            self.send_event(ConnectionEvent::Disconnected {
                device,
                reason: DisconnectReason::UserRequested,
            });
        }

        Ok(())
    }

    /// Set connection state and emit event
    async fn set_state(&self, new_state: ConnectionState) {
        let old_state = {
            let mut state = self.state.write().await;
            let old = *state;
            *state = new_state;
            old
        };

        if old_state != new_state {
            self.send_event(ConnectionEvent::StateChanged {
                old: old_state,
                new: new_state,
            });
        }
    }

    /// Send an event
    fn send_event(&self, event: ConnectionEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to connection events
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<ConnectionEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        // Forward events to subscriber
        // Note: In a real implementation, would need proper broadcast
        rx
    }
}
```

---

### 10.3 Module Entry Point

- [ ] **10.3.1** Create connection module entry point

**File:** `src/connection/mod.rs`

```rust
//! Connection management

mod state;
mod manager;

pub use state::{ConnectionState, ConnectionEvent, ConnectionStats, DisconnectReason};
pub use manager::ConnectionManager;
```

---

## Unit Tests

### Test File: `src/connection/state.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_is_active() {
        assert!(ConnectionState::Connecting.is_active());
        assert!(ConnectionState::Connected.is_active());
        assert!(!ConnectionState::Disconnected.is_active());
        assert!(!ConnectionState::Failed.is_active());
    }

    #[test]
    fn test_connection_state_is_connected() {
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
    }

    #[test]
    fn test_connection_stats() {
        let mut stats = ConnectionStats::default();
        stats.record_sent(100);
        stats.record_received(200);

        assert_eq!(stats.bytes_sent, 100);
        assert_eq!(stats.bytes_received, 200);
    }
}
```

---

## Acceptance Criteria

- [ ] Connection state machine transitions correctly
- [ ] TCP connection established with timeout
- [ ] Pairing is performed (transient or stored)
- [ ] RTSP session is initialized
- [ ] Connection events are emitted
- [ ] Disconnect cleanly tears down session
- [ ] Statistics are tracked
- [ ] All unit tests pass

---

## Notes

- Reconnection logic can be added in a future iteration
- Consider adding connection pooling for multiple devices
- TLS support may be needed for some devices
- Error recovery could be more sophisticated
