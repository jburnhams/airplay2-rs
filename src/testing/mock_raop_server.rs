//! Mock RAOP server for testing

#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]

use crate::net::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "raop")]
use crate::protocol::crypto::RaopRsaPrivateKey;
use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::broadcast;

/// Mock RAOP server state
#[derive(Debug, Clone, Default)]
pub struct MockRaopState {
    /// RTSP session ID
    pub session_id: Option<String>,
    /// Received audio packets
    pub audio_packets: Vec<Vec<u8>>,
    /// Current volume (dB)
    pub volume_db: f32,
    /// Received metadata
    pub metadata: Option<Vec<u8>>,
    /// Received artwork
    pub artwork: Option<Vec<u8>>,
    /// Current playback state
    pub playing: bool,
    /// AES key (decrypted from rsaaeskey)
    pub aes_key: Option<[u8; 16]>,
    /// AES IV
    pub aes_iv: Option<[u8; 16]>,
}

/// Mock RAOP server configuration
#[derive(Debug, Clone)]
pub struct MockRaopConfig {
    /// RTSP port (0 for dynamic)
    pub rtsp_port: u16,
    /// Audio server port (0 for dynamic)
    pub audio_port: u16,
    /// Control port (0 for dynamic)
    pub control_port: u16,
    /// Timing port (0 for dynamic)
    pub timing_port: u16,
    /// Device name
    pub name: String,
    /// MAC address
    pub mac_address: [u8; 6],
    /// Supported codecs
    pub codecs: Vec<u8>,
    /// Supported encryption types
    pub encryption_types: Vec<u8>,
    /// Require Apple-Challenge
    pub require_challenge: bool,
}

impl Default for MockRaopConfig {
    fn default() -> Self {
        Self {
            rtsp_port: 0,
            audio_port: 0,
            control_port: 0,
            timing_port: 0,
            name: "Mock RAOP".to_string(),
            mac_address: [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            codecs: vec![0, 1, 2],        // PCM, ALAC, AAC
            encryption_types: vec![0, 1], // None, RSA
            require_challenge: true,
        }
    }
}

/// Mock RAOP server
#[cfg(feature = "raop")]
pub struct MockRaopServer {
    /// Configuration
    pub config: MockRaopConfig,
    /// Server state
    pub state: Arc<Mutex<MockRaopState>>,
    /// RSA private key for authentication
    rsa_key: RaopRsaPrivateKey,
    /// Running state
    running: bool,
    /// Shutdown signal sender
    shutdown: Option<broadcast::Sender<()>>,
}

#[cfg(feature = "raop")]
impl MockRaopServer {
    /// Create new mock server
    #[must_use]
    pub fn new(config: MockRaopConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(MockRaopState::default())),
            rsa_key: RaopRsaPrivateKey::generate().expect("failed to generate RSA key"),
            running: false,
            shutdown: None,
        }
    }

    /// Get server address for connection
    #[must_use]
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.config.rtsp_port)
    }

    /// Get mDNS service name
    #[must_use]
    pub fn service_name(&self) -> String {
        let mac = self
            .config
            .mac_address
            .iter()
            .fold(String::new(), |mut acc, b| {
                use std::fmt::Write;
                write!(acc, "{b:02X}").unwrap();
                acc
            });
        format!("{mac}@{}", self.config.name)
    }

    /// Get TXT records for mDNS
    #[must_use]
    pub fn txt_records(&self) -> HashMap<String, String> {
        let mut records = HashMap::new();
        records.insert("txtvers".to_string(), "1".to_string());
        records.insert("ch".to_string(), "2".to_string());
        records.insert(
            "cn".to_string(),
            self.config
                .codecs
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        records.insert(
            "et".to_string(),
            self.config
                .encryption_types
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());
        records
    }

    /// Start the server
    #[allow(clippy::too_many_lines)]
    pub async fn start(&mut self) -> Result<(), MockServerError> {
        if self.running {
            return Ok(());
        }

        // Bind RTSP
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.config.rtsp_port))
            .await
            .map_err(|e| MockServerError::BindFailed(format!("RTSP: {e}")))?;
        if let Ok(addr) = listener.local_addr() {
            self.config.rtsp_port = addr.port();
        }

        // Bind UDP
        let audio_socket = UdpSocket::bind(format!("0.0.0.0:{}", self.config.audio_port))
            .await
            .map_err(|e| MockServerError::BindFailed(format!("Audio: {e}")))?;
        if let Ok(addr) = audio_socket.local_addr() {
            self.config.audio_port = addr.port();
        }

        let control_socket = UdpSocket::bind(format!("0.0.0.0:{}", self.config.control_port))
            .await
            .map_err(|e| MockServerError::BindFailed(format!("Control: {e}")))?;
        if let Ok(addr) = control_socket.local_addr() {
            self.config.control_port = addr.port();
        }

        let timing_socket = UdpSocket::bind(format!("0.0.0.0:{}", self.config.timing_port))
            .await
            .map_err(|e| MockServerError::BindFailed(format!("Timing: {e}")))?;
        if let Ok(addr) = timing_socket.local_addr() {
            self.config.timing_port = addr.port();
        }

        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown = Some(shutdown_tx.clone());
        self.running = true;

        let state = self.state.clone();
        let config = self.config.clone();
        let rsa_key = self.rsa_key.clone();

        // RTSP Listener
        let mut shutdown_rx_rtsp = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _)) => {
                                let state = state.clone();
                                let config = config.clone();
                                let rsa_key = rsa_key.clone();
                                tokio::spawn(async move {
                                    Self::handle_connection(stream, state, config, rsa_key).await;
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx_rtsp.recv() => {
                        break;
                    }
                }
            }
        });

        // Audio Listener
        let state_audio = self.state.clone();
        let mut shutdown_rx_audio = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                tokio::select! {
                    res = audio_socket.recv_from(&mut buf) => {
                         if let Ok((n, _)) = res {
                             let mut state = state_audio.lock().unwrap();
                             state.audio_packets.push(buf[..n].to_vec());
                         }
                    }
                    _ = shutdown_rx_audio.recv() => {
                        break;
                    }
                }
            }
        });

        // Control Listener (Dummy consume)
        let mut shutdown_rx_control = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                tokio::select! {
                    res = control_socket.recv_from(&mut buf) => {
                        // Ignore control packets for now
                        let _ = res;
                    }
                    _ = shutdown_rx_control.recv() => {
                        break;
                    }
                }
            }
        });

        // Timing Listener (Dummy consume)
        let mut shutdown_rx_timing = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                tokio::select! {
                    res = timing_socket.recv_from(&mut buf) => {
                        // Ignore timing packets for now
                        let _ = res;
                    }
                    _ = shutdown_rx_timing.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the server
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.running = false;
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> MockRaopState {
        self.state.lock().unwrap().clone()
    }

    /// Reset state
    pub fn reset(&self) {
        *self.state.lock().unwrap() = MockRaopState::default();
    }

    /// Get RSA public key (for testing client)
    #[must_use]
    pub fn public_key(&self) -> rsa::RsaPublicKey {
        self.rsa_key.public_key()
    }

    async fn handle_connection(
        mut stream: tokio::net::TcpStream,
        state: Arc<Mutex<MockRaopState>>,
        config: MockRaopConfig,
        rsa_key: RaopRsaPrivateKey,
    ) {
        let mut buffer = Vec::new();
        let mut temp_buf = vec![0u8; 4096];

        loop {
            let n = match stream.read(&mut temp_buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            buffer.extend_from_slice(&temp_buf[..n]);

            loop {
                match Self::try_parse_request(&buffer) {
                    Ok(Some((request, consumed))) => {
                        buffer.drain(..consumed);
                        let response = Self::process_request(&request, &state, &config, &rsa_key);
                        let bytes = Self::encode_response(&response);
                        if stream.write_all(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(()) => return,
                }
            }
        }
    }

    fn try_parse_request(data: &[u8]) -> Result<Option<(RtspRequest, usize)>, ()> {
        let header_end = data.windows(4).position(|w| w == b"\r\n\r\n");
        if let Some(header_end) = header_end {
            let header_len = header_end + 4;
            let header_str = String::from_utf8_lossy(&data[..header_end]);
            let mut lines = header_str.lines();
            let request_line = lines.next().ok_or(())?;
            let mut parts = request_line.split_whitespace();
            let method_str = parts.next().ok_or(())?;
            let uri = parts.next().ok_or(())?.to_string();
            let method = Method::from_str(method_str)?;

            let mut headers = Headers::new();
            let mut content_length = 0;
            for line in lines {
                if let Some(colon_pos) = line.find(':') {
                    let name = line[..colon_pos].trim().to_string();
                    let value = line[colon_pos + 1..].trim().to_string();
                    if name.eq_ignore_ascii_case("Content-Length") {
                        content_length = value.parse().unwrap_or(0);
                    }
                    headers.insert(name, value);
                }
            }

            if data.len() < header_len + content_length {
                return Ok(None);
            }
            let body = data[header_len..header_len + content_length].to_vec();
            Ok(Some((
                RtspRequest {
                    method,
                    uri,
                    headers,
                    body,
                },
                header_len + content_length,
            )))
        } else {
            Ok(None)
        }
    }

    fn encode_response(response: &crate::protocol::rtsp::RtspResponse) -> Vec<u8> {
        use std::io::Write;
        let mut bytes = Vec::new();
        write!(
            &mut bytes,
            "{} {} {}\r\n",
            response.version, response.status.0, response.reason
        )
        .unwrap();
        for (k, v) in response.headers.iter() {
            write!(&mut bytes, "{k}: {v}\r\n").unwrap();
        }
        if !response.body.is_empty() {
            write!(&mut bytes, "Content-Length: {}\r\n", response.body.len()).unwrap();
        }
        write!(&mut bytes, "\r\n").unwrap();
        bytes.extend_from_slice(&response.body);
        bytes
    }

    fn process_request(
        request: &RtspRequest,
        state: &Arc<Mutex<MockRaopState>>,
        config: &MockRaopConfig,
        rsa_key: &RaopRsaPrivateKey,
    ) -> crate::protocol::rtsp::RtspResponse {
        match request.method {
            Method::Options => Self::handle_options_static(request, config),
            Method::Announce => Self::handle_announce_static(request, state, rsa_key),
            Method::Setup => Self::handle_setup_static(request, state, config),
            Method::Record => Self::handle_record_static(request, state),
            Method::SetParameter => Self::handle_set_parameter_static(request, state),
            _ => {
                use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};
                let mut headers = Headers::new();
                if let Some(cseq) = request.headers.cseq() {
                    headers.insert("CSeq", cseq.to_string());
                }
                RtspResponse {
                    version: "RTSP/1.0".to_string(),
                    status: StatusCode::OK,
                    reason: "OK".to_string(),
                    headers,
                    body: Vec::new(),
                }
            }
        }
    }

    /// Handle RTSP OPTIONS request
    #[must_use]
    pub fn handle_options(&self, request: &RtspRequest) -> crate::protocol::rtsp::RtspResponse {
        Self::handle_options_static(request, &self.config)
    }

    fn handle_options_static(
        request: &RtspRequest,
        config: &MockRaopConfig,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Public", "ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER");

        // Handle Apple-Challenge if present
        if config.require_challenge {
            if let Some(_challenge) = request.headers.get("Apple-Challenge") {
                // Generate Apple-Response (stub)
                // In a real mock we might compute it, but for now just acknowledge
                // headers.insert("Apple-Response", "...");
            }
        }

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP ANNOUNCE request
    #[must_use]
    pub fn handle_announce(&self, request: &RtspRequest) -> crate::protocol::rtsp::RtspResponse {
        Self::handle_announce_static(request, &self.state, &self.rsa_key)
    }

    fn handle_announce_static(
        request: &RtspRequest,
        state: &Arc<Mutex<MockRaopState>>,
        rsa_key: &RaopRsaPrivateKey,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};
        use crate::protocol::sdp::SdpParser;

        // Parse SDP
        let sdp_text = String::from_utf8_lossy(&request.body);
        if let Ok(sdp) = SdpParser::parse(&sdp_text) {
            // Extract and decrypt AES key
            if let (Some(rsaaeskey), Some(aesiv)) = (sdp.rsaaeskey(), sdp.aesiv()) {
                if let Ok((key, iv)) =
                    crate::protocol::raop::parse_session_keys(rsaaeskey, aesiv, rsa_key)
                {
                    let mut state = state.lock().unwrap();
                    state.aes_key = Some(key);
                    state.aes_iv = Some(iv);
                }
            }
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP SETUP request
    #[must_use]
    pub fn handle_setup(&self, request: &RtspRequest) -> crate::protocol::rtsp::RtspResponse {
        Self::handle_setup_static(request, &self.state, &self.config)
    }

    fn handle_setup_static(
        request: &RtspRequest,
        state: &Arc<Mutex<MockRaopState>>,
        config: &MockRaopConfig,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};
        use rand::Rng;

        let session_id = format!("{:016X}", rand::thread_rng().r#gen::<u64>());

        {
            let mut state = state.lock().unwrap();
            state.session_id = Some(session_id.clone());
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Session", &session_id);
        headers.insert(
            "Transport",
            format!(
                "RTP/AVP/UDP;unicast;mode=record;server_port={};control_port={};timing_port={}",
                config.audio_port, config.control_port, config.timing_port,
            ),
        );
        headers.insert("Audio-Latency", "11025");

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP RECORD request
    #[must_use]
    pub fn handle_record(&self, request: &RtspRequest) -> crate::protocol::rtsp::RtspResponse {
        Self::handle_record_static(request, &self.state)
    }

    fn handle_record_static(
        request: &RtspRequest,
        state: &Arc<Mutex<MockRaopState>>,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};

        {
            let mut state = state.lock().unwrap();
            state.playing = true;
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());
        headers.insert("Audio-Latency", "11025");

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Handle RTSP `SET_PARAMETER` request
    #[must_use]
    pub fn handle_set_parameter(
        &self,
        request: &RtspRequest,
    ) -> crate::protocol::rtsp::RtspResponse {
        Self::handle_set_parameter_static(request, &self.state)
    }

    fn handle_set_parameter_static(
        request: &RtspRequest,
        state: &Arc<Mutex<MockRaopState>>,
    ) -> crate::protocol::rtsp::RtspResponse {
        use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};

        let content_type = request.headers.content_type().unwrap_or("");

        {
            let mut state = state.lock().unwrap();

            match content_type {
                "text/parameters" => {
                    // Parse volume
                    let body = String::from_utf8_lossy(&request.body);
                    if let Some(line) = body.lines().find(|l| l.starts_with("volume:")) {
                        if let Some(vol_str) = line.strip_prefix("volume:") {
                            if let Ok(vol) = vol_str.trim().parse::<f32>() {
                                state.volume_db = vol;
                            }
                        }
                    }
                }
                "application/x-dmap-tagged" => {
                    state.metadata = Some(request.body.clone());
                }
                "image/jpeg" | "image/png" => {
                    state.artwork = Some(request.body.clone());
                }
                _ => {}
            }
        }

        let mut headers = Headers::new();
        headers.insert("CSeq", request.headers.cseq().unwrap_or(0).to_string());

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        }
    }
}

impl Drop for MockRaopServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Mock server errors
#[derive(Debug, thiserror::Error)]
pub enum MockServerError {
    /// Failed to bind to a port
    #[error("bind failed: {0}")]
    BindFailed(String),
    /// Server is not running
    #[error("server not running")]
    NotRunning,
}
