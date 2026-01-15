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
            user_agent: format!("airplay2-rs/{}", env!("CARGO_PKG_VERSION")),
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

    /// Get next `CSeq` and increment counter
    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    /// Create base request builder with common headers
    fn request_builder(&mut self, method: Method, path: &str) -> RtspRequestBuilder {
        let uri = if path.starts_with('/') {
            format!("{}{}", self.base_uri, path)
        } else {
            format!("{}/{}", self.base_uri, path)
        };

        let mut builder = RtspRequest::builder(method, uri)
            .cseq(self.next_cseq())
            .user_agent(&self.user_agent)
            .header(names::X_APPLE_DEVICE_ID, &self.device_id)
            .header(names::X_APPLE_SESSION_ID, &self.client_session_id);

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

    /// Create SETUP request
    #[must_use]
    pub fn setup_request(&mut self, transport_params: &str) -> RtspRequest {
        self.request_builder(Method::Setup, "")
            .header(names::TRANSPORT, transport_params)
            .build()
    }

    /// Create RECORD request
    #[must_use]
    pub fn record_request(&mut self) -> RtspRequest {
        self.request_builder(Method::Record, "")
            .header("Range", "npt=0-")
            .header("RTP-Info", "seq=0;rtptime=0")
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
    pub fn get_parameter_request(&mut self) -> RtspRequest {
        self.request_builder(Method::GetParameter, "").build()
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
    #[allow(clippy::match_same_arms)]
    pub fn can_send(&self, method: Method) -> bool {
        match (self.state, method) {
            (SessionState::Init, Method::Options | Method::Post) => true,
            (SessionState::Ready, Method::Setup | Method::Post) => true,
            (SessionState::Setup, Method::Record | Method::Play) => true,
            (
                SessionState::Playing,
                Method::Pause
                | Method::Flush
                | Method::SetParameter
                | Method::GetParameter
                | Method::Teardown,
            ) => true,
            (
                SessionState::Paused,
                Method::Record | Method::Play | Method::Teardown | Method::SetParameter,
            ) => true,
            (_, Method::Options | Method::Teardown) => true, // OPTIONS and TEARDOWN always allowed
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::rtsp::{Headers, StatusCode};

    #[test]
    fn test_session_initial_state() {
        let session = RtspSession::new("192.168.1.10", 7000);

        assert_eq!(session.state(), SessionState::Init);
        assert!(session.session_id().is_none());
    }

    #[test]
    fn test_session_cseq_increments() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        let r1 = session.options_request();
        let r2 = session.options_request();

        assert_eq!(r1.headers.cseq(), Some(1));
        assert_eq!(r2.headers.cseq(), Some(2));
    }

    #[test]
    fn test_session_state_transitions() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        // Initial -> Ready via OPTIONS
        let response = RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers: Headers::new(),
            body: Vec::new(),
        };

        session
            .process_response(Method::Options, &response)
            .unwrap();
        assert_eq!(session.state(), SessionState::Ready);
    }

    #[test]
    fn test_session_extracts_session_id() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        let mut headers = Headers::new();
        headers.insert("Session", "ABC123;timeout=60");

        let response = RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::OK,
            reason: "OK".to_string(),
            headers,
            body: Vec::new(),
        };

        session.process_response(Method::Setup, &response).unwrap();

        assert_eq!(session.session_id(), Some("ABC123"));
    }

    #[test]
    fn test_session_can_send_validation() {
        let session = RtspSession::new("192.168.1.10", 7000);

        // In Init state
        assert!(session.can_send(Method::Options));
        assert!(!session.can_send(Method::Setup));
        assert!(!session.can_send(Method::Record));
    }

    #[test]
    fn test_request_includes_common_headers() {
        let mut session = RtspSession::new("192.168.1.10", 7000);
        let request = session.options_request();

        assert!(request.headers.get("X-Apple-Device-ID").is_some());
        assert!(request.headers.get("X-Apple-Session-ID").is_some());
        assert!(request.headers.get("User-Agent").is_some());
    }

    #[test]
    fn test_invalid_state_transitions() {
        let session = RtspSession::new("192.168.1.10", 7000);

        // Cannot send SETUP before OPTIONS
        assert!(!session.can_send(Method::Setup));

        // Cannot send RECORD before SETUP
        assert!(!session.can_send(Method::Record));
    }

    #[test]
    fn test_process_response_error() {
        let mut session = RtspSession::new("192.168.1.10", 7000);

        let response = RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: StatusCode::INTERNAL_ERROR,
            reason: "Internal Error".to_string(),
            headers: Headers::new(),
            body: Vec::new(),
        };

        // Should return error
        let result = session.process_response(Method::Options, &response);
        assert!(result.is_err());

        // State should not change on error
        assert_eq!(session.state(), SessionState::Init);
    }

    #[test]
    fn test_teardown_always_allowed() {
        let session = RtspSession::new("192.168.1.10", 7000);
        assert!(session.can_send(Method::Teardown));
    }
}
