//! Mock `AirPlay` server for testing purposes.
//!
//! This module provides a minimal `AirPlay` server implementation that can be used
//! to test the client functionality without requiring real hardware. It supports
//! basic RTSP negotiation, audio data reception (stub), and control commands.

use crate::net::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use crate::protocol::rtp::RtpPacket;
use crate::protocol::rtsp::{Headers, Method, RtspCodec, RtspRequest, StatusCode};

use std::fmt::Write;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

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
#[derive(Debug, Default)]
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
    /// Current state of the pairing process.
    pairing_state: u8,
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
        Self {
            config,
            state: Arc::new(RwLock::new(ServerState::default())),
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

    /// Handles a single client connection.
    async fn handle_connection(
        mut stream: TcpStream,
        state: Arc<RwLock<ServerState>>,
        config: MockServerConfig,
    ) {
        let mut codec = RtspCodec::new();
        let mut buf = vec![0u8; 4096];

        loop {
            // Read data from the stream
            let n = match stream.read(&mut buf).await {
                Ok(0) | Err(_) => break, // Connection closed or error
                Ok(n) => n,
            };

            // Feed data to the codec
            if codec.feed(&buf[..n]).is_err() {
                break;
            }

            // Process complete requests
            // Note: We use `if` instead of `while` because `parse_request` does not consume
            // the buffer, so `while` would loop infinitely on the same data.
            // This assumes one request per packet, which is sufficient for current tests.
            if let Ok(Some(request)) = Self::parse_request(&mut codec, &buf[..n]) {
                // Simulate latency if configured
                if config.latency_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(u64::from(config.latency_ms))).await;
                }

                let response = Self::handle_request(&request, &state, &config).await;

                // Write the response back to the stream
                if stream.write_all(&response).await.is_err() {
                    break;
                }
            }
        }
    }

    /// Parses an RTSP request from raw bytes using the codec.
    ///
    /// Note: The `RtspCodec` does most of the heavy lifting, but we also do some manual parsing here
    /// because the provided `RtspCodec` might be sans-IO and we need to reconstruct the request object.
    /// The logic here simplifies things by re-parsing the text from the buffer which might be inefficient
    /// but acceptable for a mock server.
    fn parse_request(_codec: &mut RtspCodec, data: &[u8]) -> Result<Option<RtspRequest>, ()> {
        // In a real implementation, we would use codec.decode().
        // Here we attempt a simplified parse for the mock.
        // However, `RtspCodec` should be used if possible.
        // Let's see if we can use the `RtspCodec` properly.
        // The original code passed `&buf[..n]` which is redundant if `feed` was called.
        // Assuming `codec.decode()` works on internal buffer.

        // The original code snippet had a manual parser implementation:
        let text = String::from_utf8_lossy(data);

        // Parse request line
        let mut lines = text.lines();
        let request_line = lines.next().ok_or(())?;
        let parts: Vec<&str> = request_line.split_whitespace().collect();

        if parts.len() < 3 {
            return Err(());
        }

        let method = Method::from_str(parts[0])?;
        let uri = parts[1].to_string();

        // Parse headers
        let mut headers = Headers::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some(pos) = line.find(':') {
                let name = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                headers.insert(name, value);
            }
        }

        // We need to handle the body too. For simplicity in this mock, we might be taking just the initial packet.
        // A proper implementation would handle Content-Length and buffering.

        // Extract body if any
        let body_start = text.find("\r\n\r\n").map_or(text.len(), |i| i + 4);
        let body = if body_start < data.len() {
            data[body_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Some(RtspRequest {
            method,
            uri,
            headers,
            body,
        }))
    }

    /// Processes a request and generates a response.
    async fn handle_request(
        request: &RtspRequest,
        state: &Arc<RwLock<ServerState>>,
        config: &MockServerConfig,
    ) -> Vec<u8> {
        let cseq = request.headers.cseq().unwrap_or(0);

        match request.method {
            Method::Options => Self::response(
                StatusCode::OK,
                cseq,
                None,
                Some("Public: SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, SET_PARAMETER, GET_PARAMETER, POST"),
            ),
            Method::Setup => {
                let mut state = state.write().await;
                state.session_id = Some(format!("{:X}", rand::random::<u64>()));
                state.streaming = false;

                let body = format!(
                    "Transport: RTP/AVP/UDP;unicast;mode=record;server_port={}-{};control_port={};timing_port={}",
                    config.audio_port, config.audio_port + 1,
                    config.control_port,
                    config.timing_port
                );

                Self::response(StatusCode::OK, cseq, state.session_id.as_deref(), Some(&body))
            }
            Method::Record => {
                state.write().await.streaming = true;
                Self::response(StatusCode::OK, cseq, None, None)
            }
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
                let volume = state.read().await.volume;
                let body = format!("volume: {volume:.6}\r\n");
                Self::response(StatusCode::OK, cseq, None, Some(&body))
            }
            Method::Post => {
                // Handle pairing
                if config.accept_pairing {
                    Self::handle_pairing(request, state).await
                } else {
                    Self::response(StatusCode::UNAUTHORIZED, cseq, None, None)
                }
            }
            _ => Self::response(StatusCode::NOT_IMPLEMENTED, cseq, None, None),
        }
    }

    /// Handles pairing requests (POST).
    async fn handle_pairing(
        request: &RtspRequest,
        state: &Arc<RwLock<ServerState>>,
    ) -> Vec<u8> {
        let cseq = request.headers.cseq().unwrap_or(0);

        // Parse TLV from body
        let Ok(tlv) = TlvDecoder::decode(&request.body) else {
            return Self::response(StatusCode::NOT_ACCEPTABLE, cseq, None, None);
        };

        let request_state = tlv.get_state().unwrap_or(0);

        let response_body = match request_state {
            1 => {
                // M1 -> M2: Send public key
                state.write().await.pairing_state = 2;
                TlvEncoder::new()
                    .add_state(2)
                    .add(TlvType::PublicKey, &[0u8; 32]) // Dummy key
                    .build()
            }
            3 => {
                // M3 -> M4: Accept and complete
                state.write().await.pairing_state = 4;
                state.write().await.paired = true;
                TlvEncoder::new()
                    .add_state(4)
                    .build()
            }
            _ => {
                return Self::response(StatusCode::NOT_ACCEPTABLE, cseq, None, None);
            }
        };

        // Re-generate response
        let mut response_vec = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: {}\r\n\r\n",
            cseq,
            response_body.len()
        ).into_bytes();

        response_vec.extend_from_slice(&response_body);
        response_vec
    }

    /// Helper to build an RTSP response.
    fn response(
        status: StatusCode,
        cseq: u32,
        session: Option<&str>,
        body: Option<&str>,
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

        if let Some(body) = body {
            let _ = write!(response, "Content-Length: {}\r\n\r\n{body}", body.len());
        } else {
            response.push_str("\r\n");
        }

        response.into_bytes()
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
