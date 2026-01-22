//! Connection manager for `AirPlay` devices

use super::state::{ConnectionEvent, ConnectionState, ConnectionStats, DisconnectReason};
use crate::error::AirPlayError;
use crate::net::{AsyncReadExt, AsyncWriteExt, Runtime, TcpStream};
use crate::protocol::pairing::{
    PairVerify, PairingKeys, PairingStepResult, PairingStorage, SessionKeys, TransientPairing,
};
use crate::protocol::rtsp::{Method, RtspCodec, RtspRequest, RtspResponse, RtspSession};
use crate::types::{AirPlayConfig, AirPlayDevice};

use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};

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
    event_tx: broadcast::Sender<ConnectionEvent>,
    /// Pairing storage
    pairing_storage: Option<Box<dyn PairingStorage>>,
}

/// UDP sockets for streaming
#[allow(dead_code)]
struct UdpSockets {
    audio: UdpSocket,
    control: UdpSocket,
    timing: UdpSocket,
    #[allow(dead_code)]
    server_audio_port: u16,
    #[allow(dead_code)]
    server_control_port: u16,
    #[allow(dead_code)]
    server_timing_port: u16,
}

impl ConnectionManager {
    /// Create a new connection manager
    #[must_use]
    pub fn new(config: AirPlayConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);

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
            pairing_storage: None,
        }
    }

    /// Set pairing storage for persistent pairing
    #[must_use]
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
    ///
    /// # Errors
    ///
    /// Returns error if connection or pairing fails
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // Check if already connected
        let current_state = *self.state.read().await;
        if current_state.is_active() {
            return Err(AirPlayError::InvalidState {
                message: "Already connected or connecting".to_string(),
                current_state: format!("{current_state:?}"),
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

        let stream =
            TcpStream::connect(&addr)
                .await
                .map_err(|e| AirPlayError::ConnectionFailed {
                    device_name: device.name.clone(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
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
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.options_request()
        };

        let response = self.send_rtsp_request(&request).await?;

        {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "RTSP session closed during OPTIONS request".to_string(),
                    current_state: "Disconnected".to_string(),
                })?;

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
        let mut pairing = TransientPairing::new();

        // M1: Start pairing
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let m2 = self.send_pairing_data(&m1, "/pair-setup").await?;

        // M2 -> M3
        let result = pairing
            .process_m2(&m2)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        if let PairingStepResult::Complete(keys) = result {
            return Ok(keys);
        }

        let PairingStepResult::SendData(m3) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state".to_string(),
                recoverable: false,
            });
        };

        let m4 = self.send_pairing_data(&m3, "/pair-setup").await?;

        // M4 -> Complete
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => Ok(keys),
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Pairing did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Perform Pair-Verify with stored keys
    async fn pair_verify(
        &self,
        _device: &AirPlayDevice,
        keys: &PairingKeys,
    ) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = PairVerify::new(keys.clone(), &keys.device_public_key).map_err(|e| {
            AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            }
        })?;

        // M1: Start verification
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let m2 = self.send_pairing_data(&m1, "/pair-verify").await?;

        // M2 -> M3
        let result = pairing
            .process_m2(&m2)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let PairingStepResult::SendData(m3) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state".to_string(),
                recoverable: false,
            });
        };

        let m4 = self.send_pairing_data(&m3, "/pair-verify").await?;

        // M4 -> Complete
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => Ok(keys),
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

        let _audio_port = audio_sock.local_addr()?.port();
        let ctrl_port = ctrl_sock.local_addr()?.port();
        let time_port = time_sock.local_addr()?.port();

        // 2. Create SETUP request with transport parameters
        // Transport: RTP/AVP/UDP;unicast;mode=record;control_port=...;timing_port=...
        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={ctrl_port};timing_port={time_port}"
        );

        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.setup_request(&transport)
        };

        // 3. Send request
        let response = self.send_rtsp_request(&request).await?;

        // 4. Update session state
        {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session
                .process_response(Method::Setup, &response)
                .map_err(|e| AirPlayError::RtspError {
                    message: e,
                    status_code: Some(response.status.as_u16()),
                })?;
        }

        // 5. Parse response transport header to get server ports
        let transport_header =
            response
                .headers
                .get("Transport")
                .ok_or(AirPlayError::RtspError {
                    message: "Missing Transport header in SETUP response".to_string(),
                    status_code: None,
                })?;

        let (server_audio_port, server_ctrl_port, server_time_port) =
            Self::parse_transport_ports(transport_header)?;

        // 6. Connect UDP sockets to server ports
        let device_ip = {
            let current_state = self.state().await;
            let device_guard = self.device.read().await;
            let device = device_guard
                .as_ref()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "Device information is missing.".to_string(),
                    current_state: format!("{current_state:?}"),
                })?;
            device.address
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
            let current_state = self.state().await;
            let mut session_guard = self.rtsp_session.lock().await;
            session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: format!("{current_state:?}"),
                })?
                .record_request()
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

        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AirPlayError::Disconnected {
                device_name: "unknown".to_string(),
            })?;

        // Send request
        stream.write_all(request.as_bytes()).await?;
        stream.write_all(data).await?;
        stream.flush().await?;

        // Read headers
        let mut buf = Vec::new();
        let mut temp_buf = [0u8; 1];
        let mut body_start = 0;

        // Read byte by byte until double CRLF (inefficient but simple for now)
        // In a real implementation, use a buffered reader or proper parser
        while body_start == 0 {
            let n = stream.read(&mut temp_buf).await?;
            if n == 0 {
                return Err(AirPlayError::RtspError {
                    message: "Connection closed while reading headers".to_string(),
                    status_code: None,
                });
            }
            buf.push(temp_buf[0]);

            if buf.len() >= 4 && buf.ends_with(b"\r\n\r\n") {
                body_start = buf.len();
            }

            if buf.len() > 4096 {
                return Err(AirPlayError::RtspError {
                    message: "Headers too large".to_string(),
                    status_code: None,
                });
            }
        }

        // Parse Content-Length
        let headers =
            std::str::from_utf8(&buf[..body_start]).map_err(|_| AirPlayError::RtspError {
                message: "Invalid UTF-8 in headers".to_string(),
                status_code: None,
            })?;

        let mut content_length = 0;
        for line in headers.lines() {
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("content-length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            }
        }

        // Read body
        let mut body = vec![0u8; content_length];
        stream.read_exact(&mut body).await?;

        Ok(body)
    }

    /// Send RTSP request and get response
    async fn send_rtsp_request(&self, request: &RtspRequest) -> Result<RtspResponse, AirPlayError> {
        let encoded = request.encode();

        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AirPlayError::Disconnected {
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
    ///
    /// # Errors
    ///
    /// Returns error if disconnection fails
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        let device = self.device.read().await.clone();

        // Send TEARDOWN if connected
        if self.state().await == ConnectionState::Connected {
            let request = {
                let mut session = self.rtsp_session.lock().await;
                session.as_mut().map(RtspSession::teardown_request)
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
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_tx.subscribe()
    }

    fn parse_transport_ports(transport_header: &str) -> Result<(u16, u16, u16), AirPlayError> {
        let mut server_audio_port = 0;
        let mut server_ctrl_port = 0;
        let mut server_time_port = 0;

        for part in transport_header.split(';') {
            if let Some((key, value)) = part.trim().split_once('=') {
                if let Ok(port) = value.parse::<u16>() {
                    match key {
                        "server_port" => server_audio_port = port,
                        "control_port" => server_ctrl_port = port,
                        "timing_port" => server_time_port = port,
                        _ => {}
                    }
                }
            }
        }

        if server_audio_port == 0 {
            return Err(AirPlayError::RtspError {
                message: "Could not determine server audio port".to_string(),
                status_code: None,
            });
        }

        Ok((server_audio_port, server_ctrl_port, server_time_port))
    }
}
