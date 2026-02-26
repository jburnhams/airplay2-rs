//! Mock `AirPlay` server for testing purposes.
//!
//! This module provides a minimal `AirPlay` server implementation that can be used
//! to test the client functionality without requiring real hardware. It supports
//! basic RTSP negotiation, audio data reception (stub), and control commands.

use std::fmt::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};

use crate::net::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::crypto::Ed25519KeyPair;
use crate::protocol::rtp::RtpPacket;
use crate::protocol::rtsp::{Headers, Method, RtspRequest, StatusCode};
use crate::receiver::ap2::PairingServer;

/// Configuration for the Mock `AirPlay` Server.
#[derive(Debug, Clone)]
pub struct MockServerConfig {
    /// Port to listen on for RTSP connections (TCP).
    pub rtsp_port: u16,
    /// Port for audio data (UDP).
    pub audio_port: u16,
    /// Port for control data (UDP).
    pub control_port: u16,
    /// Port for timing data (UDP).
    pub timing_port: u16,
    /// Name of the mock device.
    pub device_name: String,
    /// Whether to require authentication.
    pub require_auth: bool,
    /// Simulated latency in milliseconds.
    pub latency_ms: u32,
    /// Whether to accept pairing requests.
    pub accept_pairing: bool,
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            rtsp_port: 7000,
            audio_port: 6000,
            control_port: 6001,
            timing_port: 6002,
            device_name: "Mock AirPlay Device".to_string(),
            require_auth: false,
            latency_ms: 0,
            accept_pairing: true,
        }
    }
}

/// Internal state of the Mock Server.
struct ServerState {
    /// Whether the server is currently in a streaming state.
    streaming: bool,
    /// The current RTSP session ID, if any.
    session_id: Option<String>,
    /// Buffer of received audio packets.
    audio_packets: Vec<RtpPacket>,
    /// Current volume level in dB (or similar scale).
    volume: f32,
    /// Whether the client is paired.
    paired: bool,
    /// Pairing server instance
    pairing_server: PairingServer,
    /// History of received methods
    received_methods: Vec<Method>,
}

/// A Mock `AirPlay` server.
///
/// This server listens for RTSP connections and handles them according to the `AirPlay` protocol.
/// It is intended for testing clients and does not implement full audio playback.
pub struct MockServer {
    /// Server configuration.
    config: MockServerConfig,
    /// Shared server state.
    state: Arc<RwLock<ServerState>>,
    /// Channel to signal shutdown to the server task.
    shutdown: Option<mpsc::Sender<()>>,
    /// The local address the server is listening on.
    address: Option<SocketAddr>,
}

impl MockServer {
    /// Creates a new `MockServer` with the specified configuration.
    #[must_use]
    pub fn new(config: MockServerConfig) -> Self {
        let identity = Ed25519KeyPair::generate();
        let mut pairing_server = PairingServer::new(identity);
        pairing_server.set_password("3939");

        Self {
            config,
            state: Arc::new(RwLock::new(ServerState {
                streaming: false,
                session_id: None,
                audio_packets: Vec::new(),
                volume: 0.0,
                paired: false,
                pairing_server,
                received_methods: Vec::new(),
            })),
            shutdown: None,
            address: None,
        }
    }

    /// Creates a new `MockServer` with default configuration.
    #[must_use]
    pub fn default_server() -> Self {
        Self::new(MockServerConfig::default())
    }

    /// Starts the server.
    ///
    /// This spawns a background task to accept connections.
    /// Returns the socket address the server is bound to.
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP listener cannot be bound.
    pub async fn start(&mut self) -> Result<SocketAddr, std::io::Error> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.config.rtsp_port)).await?;
        let addr = listener.local_addr()?;
        self.address = Some(addr);

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown = Some(shutdown_tx);

        let state = self.state.clone();
        let config = self.config.clone();

        // Spawn the main server loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _)) => {
                                let state = state.clone();
                                let config = config.clone();
                                tokio::spawn(async move {
                                    Self::handle_connection(stream, state, config).await;
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(addr)
    }

    /// Stops the server.
    ///
    /// Signals the background task to shut down.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(()).await;
        }
    }

    /// Returns the address the server is listening on.
    #[must_use]
    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    /// Returns the number of audio packets received.
    pub async fn audio_packet_count(&self) -> usize {
        self.state.read().await.audio_packets.len()
    }

    /// Returns the current volume level.
    pub async fn volume(&self) -> f32 {
        self.state.read().await.volume
    }

    /// Checks if the server is currently streaming.
    pub async fn is_streaming(&self) -> bool {
        self.state.read().await.streaming
    }

    /// Returns the list of received methods.
    pub async fn received_methods(&self) -> Vec<Method> {
        self.state.read().await.received_methods.clone()
    }

    /// Handles a single client connection.
    async fn handle_connection(
        mut stream: TcpStream,
        state: Arc<RwLock<ServerState>>,
        config: MockServerConfig,
    ) {
        use byteorder::{ByteOrder, LittleEndian};

        use crate::net::secure::HapSecureSession;

        let mut buffer = Vec::new();
        let mut raw_buffer = Vec::new();
        let mut temp_buf = vec![0u8; 4096];
        let mut secure_session: Option<HapSecureSession> = None;

        loop {
            // Check for keys to enable encryption (only if not already enabled)
            if secure_session.is_none() {
                let state_guard = state.read().await;
                if let Some(keys) = state_guard.pairing_server.encryption_keys() {
                    // Keys in PairingServer are named from Client perspective (or derivation
                    // strings perspective): encrypt_key =
                    // Control-Write-Encryption-Key (Client Encrypts, Server Decrypts)
                    // decrypt_key = Control-Read-Encryption-Key (Client Decrypts, Server Encrypts)
                    //
                    // HapSecureSession::new(encrypt_key, decrypt_key)
                    // We are the Server. We encrypt with "Control-Read" and decrypt with
                    // "Control-Write".
                    secure_session =
                        Some(HapSecureSession::new(&keys.decrypt_key, &keys.encrypt_key));
                }
            }

            // Read data from the stream
            let n = match stream.read(&mut temp_buf).await {
                Ok(0) | Err(_) => break, // Connection closed or Error
                Ok(n) => n,
            };

            if let Some(session) = &mut secure_session {
                raw_buffer.extend_from_slice(&temp_buf[..n]);
                // Decrypt loop
                while raw_buffer.len() > 2 {
                    let length = LittleEndian::read_u16(&raw_buffer[0..2]) as usize;
                    let total_len = 2 + length + 16;

                    if raw_buffer.len() >= total_len {
                        let block = raw_buffer.drain(..total_len).collect::<Vec<_>>();
                        match session.decrypt_block(&block) {
                            Ok((plaintext, _)) => {
                                buffer.extend_from_slice(&plaintext);
                            }
                            Err(e) => {
                                tracing::error!("Decryption failed: {}", e);
                                return;
                            }
                        }
                    } else {
                        break;
                    }
                }
            } else {
                buffer.extend_from_slice(&temp_buf[..n]);
            }

            // Try to parse requests loop (in case multiple requests are buffered)
            loop {
                match Self::try_parse_request(&buffer) {
                    Ok(Some((request, consumed))) => {
                        // Remove consumed bytes
                        buffer.drain(..consumed);

                        // Simulate latency if configured
                        if config.latency_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(u64::from(config.latency_ms)))
                                .await;
                        }

                        // Determine if we should encrypt response based on CURRENT session state
                        // (before processing which might update keys)
                        // Actually, if secure_session is set, we encrypt.
                        let was_encrypted = secure_session.is_some();

                        let response = Self::handle_request(&request, &state, &config).await;

                        if was_encrypted {
                            if let Some(session) = &mut secure_session {
                                match session.encrypt(&response) {
                                    Ok(encrypted) => {
                                        if stream.write_all(&encrypted).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Encryption failed: {}", e);
                                        return;
                                    }
                                }
                            }
                        } else if stream.write_all(&response).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => break, // Need more data
                    Err(()) => {
                        // Parse error, clear buffer or disconnect?
                        // For now disconnect
                        // Reset pairing state on disconnect to allow new connections to start fresh
                        state.write().await.pairing_server.reset();
                        return;
                    }
                }
            }
        }

        // Connection closed cleanly
        state.write().await.pairing_server.reset();
    }

    /// Tries to parse an RTSP request from the buffer.
    ///
    /// Returns `Ok(Some((request, consumed_bytes)))` if a complete request is found.
    /// Returns `Ok(None)` if more data is needed.
    /// Returns `Err(())` if parsing fails.
    fn try_parse_request(data: &[u8]) -> Result<Option<(RtspRequest, usize)>, ()> {
        // Find end of headers
        let header_end = data.windows(4).position(|w| w == b"\r\n\r\n");

        if let Some(header_end) = header_end {
            let header_len = header_end + 4;
            let header_bytes = &data[..header_end];
            // Use lossy conversion to string for header parsing
            let header_str = String::from_utf8_lossy(header_bytes);

            let mut lines = header_str.lines();
            let request_line = lines.next().ok_or(())?;
            let mut parts = request_line.split_whitespace();
            let method_str = parts.next().ok_or(())?;
            let uri = parts.next().ok_or(())?.to_string();
            // version is likely RTSP/1.0 but we ignore it for mock

            let method = Method::from_str(method_str)?;

            let mut rtsp_headers = Headers::new();
            let mut content_length = 0;

            for line in lines {
                if let Some(colon_pos) = line.find(':') {
                    let name = line[..colon_pos].trim().to_string();
                    let value = line[colon_pos + 1..].trim().to_string();

                    if name.eq_ignore_ascii_case("Content-Length") {
                        content_length = value.parse::<usize>().unwrap_or(0);
                    }
                    rtsp_headers.insert(name, value);
                }
            }

            // Check if we have the full body
            if data.len() < header_len + content_length {
                return Ok(None);
            }

            let body = data[header_len..header_len + content_length].to_vec();

            Ok(Some((
                RtspRequest {
                    method,
                    uri,
                    headers: rtsp_headers,
                    body,
                },
                header_len + content_length,
            )))
        } else {
            Ok(None)
        }
    }

    /// Processes a request and generates a response.
    #[allow(clippy::too_many_lines)]
    async fn handle_request(
        request: &RtspRequest,
        state: &Arc<RwLock<ServerState>>,
        config: &MockServerConfig,
    ) -> Vec<u8> {
        let cseq = request.headers.cseq().unwrap_or(0);

        // Record method
        state.write().await.received_methods.push(request.method);

        match request.method {
            Method::Options => Self::response(
                StatusCode::OK,
                cseq,
                None,
                Some(
                    "Public: SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, SET_PARAMETER, \
                     GET_PARAMETER, POST, PLAY",
                ),
            ),
            Method::Setup => {
                let mut state = state.write().await;
                state.session_id = Some(format!("{:X}", rand::random::<u64>()));
                state.streaming = false;

                let transport = format!(
                    "RTP/AVP/UDP;unicast;mode=record;server_port={};control_port={};timing_port={}",
                    config.audio_port,
                    // config.audio_port + 1,
                    config.control_port,
                    config.timing_port
                );

                let session_id = state.session_id.clone().unwrap();

                let response = format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: {session_id}\r\nTransport: \
                     {transport}\r\n\r\n",
                );

                response.into_bytes()
            }
            Method::Record | Method::Play => {
                state.write().await.streaming = true;
                Self::response(StatusCode::OK, cseq, None, None)
            }
            Method::SetRateAnchorTime => {
                // Parse body to check rate
                let streaming = if let Ok(plist) = crate::protocol::plist::decode(&request.body) {
                    if let Some(dict) = plist.as_dict() {
                        if let Some(rate) = dict
                            .get("rate")
                            .and_then(crate::protocol::plist::PlistValue::as_f64)
                        {
                            rate.abs() > f64::EPSILON
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                state.write().await.streaming = streaming;
                Self::response(StatusCode::OK, cseq, None, None)
            }
            Method::SetPeers => Self::response(StatusCode::OK, cseq, None, None),
            Method::Pause => {
                state.write().await.streaming = false;
                Self::response(StatusCode::OK, cseq, None, None)
            }
            Method::Teardown => {
                let mut state = state.write().await;
                state.streaming = false;
                state.session_id = None;
                Self::response(StatusCode::OK, cseq, None, None)
            }
            Method::SetParameter => {
                // Parse volume if present
                let body_str = String::from_utf8_lossy(&request.body);
                if let Some(vol_line) = body_str.lines().find(|l| l.starts_with("volume:")) {
                    if let Some(vol) = vol_line.split(':').nth(1) {
                        if let Ok(v) = vol.trim().parse::<f32>() {
                            state.write().await.volume = v;
                        }
                    }
                }
                Self::response(StatusCode::OK, cseq, None, None)
            }
            Method::GetParameter => {
                if request.uri.ends_with("/info") {
                    use std::collections::HashMap;

                    use crate::protocol::plist::{PlistValue, encode};

                    let mut dict = HashMap::new();
                    dict.insert(
                        "manufacturer".to_string(),
                        PlistValue::String("OpenAirplay".to_string()),
                    );
                    dict.insert(
                        "model".to_string(),
                        PlistValue::String("MockServer".to_string()),
                    );
                    dict.insert(
                        "name".to_string(),
                        PlistValue::String("Mock Device".to_string()),
                    );
                    // Add supported features (AirPlay 2, Audio, etc.)
                    // Bit 48 (AirPlay 2), Bit 9 (Audio) -> 1<<48 | 1<<9
                    let features: u64 = (1 << 48) | (1 << 9) | (1 << 40); // + PTP
                    dict.insert(
                        "features".to_string(),
                        PlistValue::UnsignedInteger(features),
                    );

                    let body = encode(&PlistValue::Dictionary(dict)).unwrap_or_default();
                    Self::response_binary(
                        StatusCode::OK,
                        cseq,
                        None,
                        Some(&body),
                        Some("application/x-apple-binary-plist"),
                    )
                } else {
                    let volume = state.read().await.volume;
                    let body = format!("volume: {volume:.6}\r\n");
                    Self::response(StatusCode::OK, cseq, None, Some(&body))
                }
            }
            Method::Post => {
                if request.uri.ends_with("/auth-setup") {
                    // Just accept auth-setup with OK
                    // Use binary 32 bytes (curve25519 public key stub)
                    let body = [0u8; 32];
                    Self::response_binary(
                        StatusCode::OK,
                        cseq,
                        None,
                        Some(&body),
                        Some("application/octet-stream"),
                    )
                } else if request.uri.ends_with("/pair-setup")
                    || request.uri.ends_with("/pair-verify")
                {
                    if config.accept_pairing {
                        Self::handle_pairing(request, state).await
                    } else {
                        Self::response(StatusCode::UNAUTHORIZED, cseq, None, None)
                    }
                } else {
                    // Generic POST command (like /next, /prev, /ctrl-int/1/nextitem)
                    // Just accept it for mock
                    Self::response(StatusCode::OK, cseq, None, None)
                }
            }
            _ => Self::response(StatusCode::NOT_IMPLEMENTED, cseq, None, None),
        }
    }

    /// Handles pairing requests (POST).
    async fn handle_pairing(request: &RtspRequest, state: &Arc<RwLock<ServerState>>) -> Vec<u8> {
        let cseq = request.headers.cseq().unwrap_or(0);
        let mut state_guard = state.write().await;

        let result = if request.uri.ends_with("/pair-setup") {
            state_guard.pairing_server.process_pair_setup(&request.body)
        } else if request.uri.ends_with("/pair-verify") {
            state_guard
                .pairing_server
                .process_pair_verify(&request.body)
        } else {
            return Self::response(StatusCode::NOT_FOUND, cseq, None, None);
        };

        if result.complete {
            state_guard.paired = true;
        }

        // Re-generate response
        let mut response_vec = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: {}\r\nContent-Type: \
             application/pairing+tlv8\r\n\r\n",
            cseq,
            result.response.len()
        )
        .into_bytes();

        response_vec.extend_from_slice(&result.response);
        response_vec
    }

    /// Helper to build an RTSP response.
    fn response(
        status: StatusCode,
        cseq: u32,
        session: Option<&str>,
        body: Option<&str>,
    ) -> Vec<u8> {
        Self::response_binary(
            status,
            cseq,
            session,
            body.map(str::as_bytes),
            Some("text/parameters"),
        )
    }

    /// Helper to build an RTSP response with binary body.
    fn response_binary(
        status: StatusCode,
        cseq: u32,
        session: Option<&str>,
        body: Option<&[u8]>,
        content_type: Option<&str>,
    ) -> Vec<u8> {
        let reason = match status.0 {
            200 => "OK",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            _ => "Unknown",
        };

        let mut response = format!("RTSP/1.0 {} {}\r\nCSeq: {}\r\n", status.0, reason, cseq);

        if let Some(session) = session {
            let _ = write!(response, "Session: {session}\r\n");
        }

        if let Some(ct) = content_type {
            let _ = write!(response, "Content-Type: {ct}\r\n");
        }

        if let Some(body) = body {
            let _ = write!(response, "Content-Length: {}\r\n\r\n", body.len());
            let mut v = response.into_bytes();
            v.extend_from_slice(body);
            v
        } else {
            response.push_str("\r\n");
            response.into_bytes()
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        // Trigger shutdown
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.try_send(());
        }
    }
}
