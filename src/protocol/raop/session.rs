//! RAOP RTSP session management

use super::auth::RaopAuthenticator;
use super::key_exchange::RaopSessionKeys;
use crate::protocol::rtsp::{
    Method, RtspRequest, RtspRequestBuilder, RtspResponse, headers::names, headers::raop,
};

/// RAOP session states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaopSessionState {
    /// Initial state
    Init,
    /// OPTIONS sent, checking authentication
    OptionsExchange,
    /// ANNOUNCE sent with stream params
    Announcing,
    /// SETUP sent, configuring transport
    SettingUp,
    /// RECORD sent, streaming
    Recording,
    /// Paused (FLUSH sent)
    Paused,
    /// Session terminated
    Terminated,
}

/// Transport configuration from SETUP
#[derive(Debug, Clone)]
pub struct RaopTransport {
    /// Server audio data port
    pub server_port: u16,
    /// Server control port
    pub control_port: u16,
    /// Server timing port
    pub timing_port: u16,
    /// Client control port
    pub client_control_port: u16,
    /// Client timing port
    pub client_timing_port: u16,
}

/// RAOP RTSP session manager
pub struct RaopRtspSession {
    /// Current state
    state: RaopSessionState,
    /// `CSeq` counter
    cseq: u32,
    /// Server session ID
    pub(crate) session_id: Option<String>,
    /// Client instance ID (64-bit hex)
    pub(crate) client_instance: String,
    /// DACP ID for remote control
    dacp_id: String,
    /// Active remote token
    active_remote: String,
    /// Server address
    server_addr: String,
    /// Server port
    server_port: u16,
    /// Authentication state
    authenticator: RaopAuthenticator,
    /// Session encryption keys
    session_keys: Option<RaopSessionKeys>,
    /// Transport configuration
    transport: Option<RaopTransport>,
    /// Audio latency (samples)
    audio_latency: u32,
}

impl RaopRtspSession {
    /// Create a new RAOP session
    #[must_use]
    pub fn new(server_addr: &str, server_port: u16) -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        Self {
            state: RaopSessionState::Init,
            cseq: 0,
            session_id: None,
            client_instance: format!("{:016X}", rng.r#gen::<u64>()),
            dacp_id: format!("{:016X}", rng.r#gen::<u64>()),
            active_remote: rng.r#gen::<u32>().to_string(),
            server_addr: server_addr.to_string(),
            server_port,
            authenticator: RaopAuthenticator::new(),
            session_keys: None,
            transport: None,
            audio_latency: 11025, // Default ~250ms at 44.1kHz
        }
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> RaopSessionState {
        self.state
    }

    /// Get transport configuration
    #[must_use]
    pub fn transport(&self) -> Option<&RaopTransport> {
        self.transport.as_ref()
    }

    /// Get session keys
    #[must_use]
    pub fn session_keys(&self) -> Option<&RaopSessionKeys> {
        self.session_keys.as_ref()
    }

    /// Get session ID
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Get next `CSeq`
    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    /// Get base URI
    fn uri(&self, path: &str) -> String {
        if path.is_empty() {
            format!(
                "rtsp://{}:{}/{}",
                self.server_addr, self.server_port, self.client_instance
            )
        } else {
            format!("rtsp://{}:{}/{}", self.server_addr, self.server_port, path)
        }
    }

    /// Add common headers to request
    fn add_common_headers(&self, builder: RtspRequestBuilder, cseq: u32) -> RtspRequestBuilder {
        let mut b = builder
            .cseq(cseq)
            .header(names::USER_AGENT, "iTunes/12.0 (Macintosh)")
            .header(raop::CLIENT_INSTANCE, &self.client_instance)
            .header(raop::DACP_ID, &self.dacp_id)
            .header(raop::ACTIVE_REMOTE, &self.active_remote);

        if let Some(ref session) = self.session_id {
            b = b.session(session);
        }

        b
    }

    /// Create OPTIONS request
    pub fn options_request(&mut self) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Options, self.uri("*"));

        self.add_common_headers(builder, cseq)
            .header(raop::APPLE_CHALLENGE, self.authenticator.challenge_header())
            .build()
    }

    /// Create ANNOUNCE request with SDP
    pub fn announce_request(&mut self, sdp: &str) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Announce, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "application/sdp")
            .body(sdp.as_bytes().to_vec())
            .build()
    }

    /// Create SETUP request
    pub fn setup_request(&mut self, control_port: u16, timing_port: u16) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Setup, self.uri(""));

        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={control_port};timing_port={timing_port}"
        );

        self.add_common_headers(builder, cseq)
            .header(names::TRANSPORT, &transport)
            .build()
    }

    /// Create RECORD request
    pub fn record_request(&mut self, seq: u16, rtptime: u32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Record, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(raop::RANGE, "npt=0-")
            .header("RTP-Info", format!("seq={seq};rtptime={rtptime}"))
            .build()
    }

    /// Create `SET_PARAMETER` request for volume
    pub fn set_volume_request(&mut self, volume_db: f32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        let body = format!("volume: {volume_db:.6}\r\n");

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "text/parameters")
            .body(body.into_bytes())
            .build()
    }

    /// Create `SET_PARAMETER` request for progress
    pub fn set_progress_request(&mut self, start: u32, current: u32, end: u32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        let body = format!("progress: {start}/{current}/{end}\r\n");

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "text/parameters")
            .body(body.into_bytes())
            .build()
    }

    /// Create FLUSH request
    pub fn flush_request(&mut self, seq: u16, rtptime: u32) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Flush, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header("RTP-Info", format!("seq={seq};rtptime={rtptime}"))
            .build()
    }

    /// Create TEARDOWN request
    pub fn teardown_request(&mut self) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::Teardown, self.uri(""));

        self.add_common_headers(builder, cseq).build()
    }

    /// Process response and update state
    ///
    /// # Errors
    ///
    /// Returns `String` (error message) if response indicates failure or is invalid.
    pub fn process_response(
        &mut self,
        method: Method,
        response: &RtspResponse,
    ) -> Result<(), String> {
        if !response.is_success() {
            return Err(format!(
                "{} failed: {} {}",
                method.as_str(),
                response.status.as_u16(),
                response.reason
            ));
        }

        // Extract session ID
        if let Some(session) = response.session() {
            let session_id = session.split(';').next().unwrap_or(session);
            self.session_id = Some(session_id.to_string());
        }

        match method {
            Method::Options => {
                // Verify Apple-Response if present
                if let Some(_apple_response) = response.headers.get(raop::APPLE_RESPONSE) {
                    // TODO: Verify with known server parameters
                    // For now, accept any response
                }
                self.authenticator.mark_sent();
                self.state = RaopSessionState::OptionsExchange;
            }
            Method::Announce => {
                self.state = RaopSessionState::Announcing;
            }
            Method::Setup => {
                // Parse transport response
                if let Some(transport) = response.headers.get(names::TRANSPORT) {
                    self.transport = Some(Self::parse_transport(transport)?);
                }
                // Extract audio latency
                if let Some(latency) = response.headers.get(raop::AUDIO_LATENCY) {
                    self.audio_latency = latency.parse().unwrap_or(11025);
                }
                self.state = RaopSessionState::SettingUp;
            }
            Method::Record => {
                self.state = RaopSessionState::Recording;
            }
            Method::Flush => {
                self.state = RaopSessionState::Paused;
            }
            Method::Teardown => {
                self.state = RaopSessionState::Terminated;
            }
            _ => {}
        }

        Ok(())
    }

    pub(crate) fn parse_transport(transport: &str) -> Result<RaopTransport, String> {
        // Parse transport header like:
        // RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002

        let mut server_port = 0u16;
        let mut control_port = 0u16;
        let mut timing_port = 0u16;

        for part in transport.split(';') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "server_port" => server_port = value.parse().unwrap_or(0),
                    "control_port" => control_port = value.parse().unwrap_or(0),
                    "timing_port" => timing_port = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if server_port == 0 {
            return Err("missing server_port in transport".to_string());
        }

        Ok(RaopTransport {
            server_port,
            control_port,
            timing_port,
            client_control_port: 0, // Set by caller
            client_timing_port: 0,
        })
    }

    /// Generate session keys and prepare ANNOUNCE
    ///
    /// # Errors
    ///
    /// Returns `String` error if key generation fails.
    pub fn prepare_announce(&mut self) -> Result<String, String> {
        let keys = RaopSessionKeys::generate().map_err(|e| e.to_string())?;

        let sdp = crate::protocol::sdp::create_raop_announce_sdp(
            &self.client_instance,
            "0.0.0.0", // Will be filled by actual client IP
            &self.server_addr,
            &keys.rsaaeskey(),
            &keys.aesiv(),
        );

        self.session_keys = Some(keys);
        Ok(sdp)
    }
}
