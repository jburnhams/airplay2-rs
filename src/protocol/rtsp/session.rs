use super::{Method, RtspRequest, RtspRequestBuilder, RtspResponse, headers::names};

/// RTSP session states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state, no session established
    Init,
    /// OPTIONS exchanged, ready for setup
    Ready,
    /// SETUP complete, transport configured
    Setup,
    /// RECORD/PLAY started, streaming active
    Playing,
    /// Paused
    Paused,
    /// Session terminated
    Terminated,
}

/// RTSP session manager (sans-IO)
///
/// Manages session state, `CSeq` numbering, and session ID tracking.
pub struct RtspSession {
    /// Current session state
    state: SessionState,
    /// `CSeq` counter
    cseq: u32,
    /// Session ID (from server)
    session_id: Option<String>,
    /// Our device ID
    device_id: String,
    /// Our session ID (generated)
    client_session_id: String,
    /// Base URI for requests
    base_uri: String,
    /// User agent string
    user_agent: String,
}

impl RtspSession {
    /// Create a new session
    #[must_use]
    pub fn new(device_address: &str, port: u16) -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let device_id: u64 = rng.r#gen();
        let session_id: u64 = rng.r#gen();

        Self {
            state: SessionState::Init,
            cseq: 0,
            session_id: None,
            device_id: format!("{device_id:016X}"),
            client_session_id: format!("{session_id:016X}"),
            base_uri: format!("rtsp://{device_address}:{port}"),
            user_agent: "AirPlay/540.31".to_string(),
        }
    }

    /// Get current session state
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get server session ID (if established)
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Get client session ID
    #[must_use]
    pub fn client_session_id(&self) -> &str {
        &self.client_session_id
    }

    /// Get device ID
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Get user agent
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Get next `CSeq` and increment counter
    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    /// Create base request builder with common headers
    fn request_builder(&mut self, method: Method, path: &str) -> RtspRequestBuilder {
        let uri = if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        };

        let mut builder = RtspRequest::builder(method, uri)
            .cseq(self.next_cseq())
            .user_agent(&self.user_agent)
            .header(names::X_APPLE_DEVICE_ID, &self.device_id)
            .header(names::X_APPLE_SESSION_ID, &self.client_session_id)
            .header(names::ACTIVE_REMOTE, "4294967295")
            .header(names::DACP_ID, &self.device_id)
            .header(names::CLIENT_INSTANCE, &self.device_id);

        if let Some(ref session) = self.session_id {
            builder = builder.session(session);
        }

        builder
    }

    /// Create OPTIONS request
    #[must_use]
    pub fn options_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Options, "*").build()
    }

    /// Create SETUP request with plist body (Session Setup)
    #[must_use]
    pub fn setup_session_request(
        &mut self,
        plist: &crate::protocol::plist::PlistValue,
        transport: Option<&str>,
    ) -> RtspRequest {
        // Per airplay2-homepod.md, SETUP requires session ID in URL: rtsp://<host>/<session-id>
        let path = format!("/{}", self.client_session_id);
        let mut builder = self.request_builder(Method::Setup, &path).body_plist(plist);

        if let Some(t) = transport {
            builder = builder.header(names::TRANSPORT, t);
        }

        builder.build()
    }

    /// Create ANNOUNCE request with SDP
    #[must_use]
    pub fn announce_request(&mut self, sdp: &str) -> RtspRequest {
        self.request_builder(Method::Announce, "/")
            .content_type("application/sdp")
            .body(sdp.as_bytes().to_vec())
            .build()
    }

    /// Create SETUP request for audio stream
    #[must_use]
    pub fn setup_stream_request(&mut self, transport_params: &str) -> RtspRequest {
        self.request_builder(Method::Setup, "/rtp/audio")
            .header(names::TRANSPORT, transport_params)
            .build()
    }

    /// Create RECORD request
    #[must_use]
    pub fn record_request(&mut self) -> RtspRequest {
        let path = format!("/{}", self.client_session_id);
        self.request_builder(Method::Record, &path).build()
    }

    /// Create PLAY request
    #[must_use]
    pub fn play_request(&mut self, content_type: &str, body: Vec<u8>) -> RtspRequest {
        self.request_builder(Method::Play, "")
            .content_type(content_type)
            .body(body)
            .build()
    }

    /// Create SETPEERS request for PTP timing peer list
    #[must_use]
    pub fn set_peers_request(&mut self, body: Vec<u8>) -> RtspRequest {
        let path = format!("/{}", self.client_session_id);
        self.request_builder(Method::SetPeers, &path)
            .content_type("/peer-list-changed")
            .body(body)
            .build()
    }

    /// Create `SET_PARAMETER` request
    #[must_use]
    pub fn set_parameter_request(&mut self, content_type: &str, body: Vec<u8>) -> RtspRequest {
        self.request_builder(Method::SetParameter, "")
            .content_type(content_type)
            .body(body)
            .build()
    }

    /// Create `GET_PARAMETER` request
    #[must_use]
    pub fn get_parameter_request(
        &mut self,
        content_type: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> RtspRequest {
        let mut builder = self.request_builder(Method::GetParameter, "");

        if let Some(ct) = content_type {
            builder = builder.content_type(ct);
        }

        if let Some(b) = body {
            builder = builder.body(b);
        }

        builder.build()
    }

    /// Create FLUSH request
    #[must_use]
    pub fn flush_request(&mut self, seq: u16, timestamp: u32) -> RtspRequest {
        self.request_builder(Method::Flush, "")
            .header("RTP-Info", format!("seq={seq};rtptime={timestamp}"))
            .build()
    }

    /// Create TEARDOWN request
    #[must_use]
    pub fn teardown_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Teardown, "").build()
    }

    /// Create PAUSE request
    #[must_use]
    pub fn pause_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Pause, "").build()
    }

    /// Create SETRATEANCHORTIME request
    #[must_use]
    pub fn set_rate_anchor_time_request(
        &mut self,
        content_type: &str,
        body: Vec<u8>,
    ) -> RtspRequest {
        let path = format!("/{}", self.client_session_id);
        self.request_builder(Method::SetRateAnchorTime, &path)
            .content_type(content_type)
            .body(body)
            .build()
    }

    /// Create POST request
    #[must_use]
    pub fn post_request(&mut self, path: &str, content_type: &str, body: Vec<u8>) -> RtspRequest {
        self.request_builder(Method::Post, path)
            .content_type(content_type)
            .body(body)
            .build()
    }

    /// Create GET request
    #[must_use]
    pub fn get_request(&mut self, path: &str) -> RtspRequest {
        self.request_builder(Method::Get, path).build()
    }

    /// Process a response and update session state
    ///
    /// Returns Ok(()) if response is valid, Err with description otherwise.
    ///
    /// # Errors
    /// Returns an error string if the response status code is not success.
    pub fn process_response(
        &mut self,
        method: Method,
        response: &RtspResponse,
    ) -> Result<(), String> {
        // Validate CSeq matches (optional, for debugging)

        if !response.is_success() {
            return Err(format!(
                "{} failed: {} {}",
                method.as_str(),
                response.status.as_u16(),
                response.reason
            ));
        }

        // Extract session ID if present
        if let Some(session) = response.session() {
            // Session ID may have ";timeout=X" suffix
            let session_id = session.split(';').next().unwrap_or(session);
            self.session_id = Some(session_id.to_string());
        }

        // Update state based on method
        match method {
            Method::Options => {
                self.state = SessionState::Ready;
            }
            Method::Setup => {
                self.state = SessionState::Setup;
            }
            Method::Record | Method::Play => {
                self.state = SessionState::Playing;
            }
            Method::Pause => {
                self.state = SessionState::Paused;
            }
            Method::Teardown => {
                self.state = SessionState::Terminated;
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a method is valid in current state
    #[must_use]
    pub fn can_send(&self, method: Method) -> bool {
        matches!(
            (self.state, method),
            (SessionState::Init, Method::Options | Method::Post)
                | (SessionState::Ready, Method::Setup | Method::Post)
                | (SessionState::Setup, Method::Record | Method::Play)
                | (
                    SessionState::Playing,
                    Method::Pause
                        | Method::Flush
                        | Method::SetParameter
                        | Method::GetParameter
                        | Method::Teardown,
                )
                | (
                    SessionState::Paused,
                    Method::Record
                        | Method::Play
                        | Method::Teardown
                        | Method::SetParameter
                        | Method::SetRateAnchorTime,
                )
                | (
                    _,
                    Method::Options
                        | Method::Teardown
                        | Method::Get
                        | Method::SetRateAnchorTime
                        | Method::SetPeers
                )
        )
    }
}
