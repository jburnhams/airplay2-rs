//! Receiver session management
//!
//! Manages the lifecycle of an `AirPlay` streaming session from
//! connection through teardown.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Session states following RAOP protocol flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state after TCP connection
    Connected,
    /// ANNOUNCE received, stream parameters known
    Announced,
    /// SETUP complete, UDP ports allocated
    Setup,
    /// RECORD received, actively streaming
    Streaming,
    /// PAUSE received, stream paused but session alive
    Paused,
    /// TEARDOWN received or connection lost
    Teardown,
    /// Session ended, ready for cleanup
    Closed,
}

impl SessionState {
    /// Check if transition to new state is valid
    #[must_use]
    pub fn can_transition_to(&self, new_state: SessionState) -> bool {
        use SessionState::{Announced, Closed, Connected, Paused, Setup, Streaming, Teardown};

        match (self, new_state) {
            // Valid transitions
            (Connected | Announced | Setup, Announced)
            | (Announced | Setup, Setup)
            | (Setup | Paused, Streaming)
            | (Streaming, Paused)
            | (Connected | Announced | Setup | Streaming | Paused, Teardown)
            | (Teardown, Closed) => true,

            _ => false,
        }
    }

    /// Is this an active streaming state?
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, SessionState::Streaming | SessionState::Paused)
    }

    /// Is the session still valid (not closed)?
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !matches!(self, SessionState::Teardown | SessionState::Closed)
    }
}

/// Stream parameters parsed from ANNOUNCE SDP
#[derive(Debug, Clone)]
pub struct StreamParameters {
    /// Audio codec
    pub codec: AudioCodec,
    /// Sample rate (typically 44100)
    pub sample_rate: u32,
    /// Bits per sample (typically 16)
    pub bits_per_sample: u8,
    /// Number of channels (typically 2)
    pub channels: u8,
    /// Samples per RTP packet (typically 352)
    pub frames_per_packet: u32,
    /// AES key (decrypted from RSA, if encryption used)
    pub aes_key: Option<[u8; 16]>,
    /// AES IV (if encryption used)
    pub aes_iv: Option<[u8; 16]>,
    /// Minimum latency requested by sender (in samples)
    pub min_latency: Option<u32>,
}

/// Audio codecs supported by `AirPlay`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// PCM (L16)
    Pcm,
    /// Apple Lossless (ALAC)
    Alac,
    /// AAC Low Complexity
    AacLc,
    /// AAC Enhanced Low Delay
    AacEld,
}

impl Default for StreamParameters {
    fn default() -> Self {
        Self {
            codec: AudioCodec::Alac,
            sample_rate: 44_100,
            bits_per_sample: 16,
            channels: 2,
            frames_per_packet: 352,
            aes_key: None,
            aes_iv: None,
            min_latency: None,
        }
    }
}

/// UDP socket addresses for a session
#[derive(Debug, Clone)]
pub struct SessionSockets {
    /// Our audio receive port
    pub audio_port: u16,
    /// Our control port (sync packets)
    pub control_port: u16,
    /// Our timing port (NTP-like)
    pub timing_port: u16,
    /// Client's control port (for sending retransmit requests)
    pub client_control_port: Option<u16>,
    /// Client's timing port
    pub client_timing_port: Option<u16>,
    /// Client's address
    pub client_addr: Option<SocketAddr>,
}

/// A receiver session
#[derive(Debug)]
pub struct ReceiverSession {
    /// Unique session identifier
    id: String,
    /// Current state
    state: SessionState,
    /// Client address
    client_addr: SocketAddr,
    /// Stream parameters (set after ANNOUNCE)
    stream_params: Option<StreamParameters>,
    /// Socket configuration (set after SETUP)
    sockets: Option<SessionSockets>,
    /// Current volume (-144.0 to 0.0 dB)
    volume: f32,
    /// Last activity timestamp
    last_activity: Instant,
    /// Session creation time
    created_at: Instant,
    /// RTSP session ID sent to client
    rtsp_session_id: Option<String>,
    /// Initial RTP sequence number
    initial_seq: Option<u16>,
    /// Initial RTP timestamp
    initial_rtptime: Option<u32>,
}

impl ReceiverSession {
    /// Create a new session
    #[must_use]
    pub fn new(client_addr: SocketAddr) -> Self {
        Self {
            id: generate_session_id(),
            state: SessionState::Connected,
            client_addr,
            stream_params: None,
            sockets: None,
            volume: 0.0, // Full volume
            last_activity: Instant::now(),
            created_at: Instant::now(),
            rtsp_session_id: None,
            initial_seq: None,
            initial_rtptime: None,
        }
    }

    /// Get session ID
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Set state (validates transition)
    ///
    /// # Errors
    /// Returns `SessionError::InvalidTransition` if the state transition is not allowed.
    pub fn set_state(&mut self, new_state: SessionState) -> Result<(), SessionError> {
        if !self.state.can_transition_to(new_state) {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: new_state,
            });
        }
        self.state = new_state;
        self.touch();
        Ok(())
    }

    /// Get client address
    #[must_use]
    pub fn client_addr(&self) -> SocketAddr {
        self.client_addr
    }

    /// Get volume in dB
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Set volume in dB (-144.0 to 0.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(-144.0, 0.0);
        self.touch();
    }

    /// Set stream parameters (from ANNOUNCE)
    pub fn set_stream_params(&mut self, params: StreamParameters) {
        self.stream_params = Some(params);
        self.touch();
    }

    /// Get stream parameters
    #[must_use]
    pub fn stream_params(&self) -> Option<&StreamParameters> {
        self.stream_params.as_ref()
    }

    /// Set socket configuration (from SETUP)
    pub fn set_sockets(&mut self, sockets: SessionSockets) {
        self.sockets = Some(sockets);
        self.touch();
    }

    /// Get socket configuration
    #[must_use]
    pub fn sockets(&self) -> Option<&SessionSockets> {
        self.sockets.as_ref()
    }

    /// Set RTSP session ID
    pub fn set_rtsp_session_id(&mut self, id: String) {
        self.rtsp_session_id = Some(id);
    }

    /// Get RTSP session ID
    #[must_use]
    pub fn rtsp_session_id(&self) -> Option<&str> {
        self.rtsp_session_id.as_deref()
    }

    /// Set initial RTP info (from RECORD)
    pub fn set_rtp_info(&mut self, seq: u16, rtptime: u32) {
        self.initial_seq = Some(seq);
        self.initial_rtptime = Some(rtptime);
        self.touch();
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Get time since last activity
    #[must_use]
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Get session age
    #[must_use]
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Check if session has timed out
    #[must_use]
    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.idle_time() > timeout
    }
}

/// Session errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// State transition is not allowed
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Current state
        from: SessionState,
        /// Target state
        to: SessionState,
    },

    /// Session ID not found
    #[error("Session not found: {0}")]
    NotFound(String),

    /// Server is busy with another session
    #[error("Session busy: another session is active")]
    Busy,

    /// Session has timed out due to inactivity
    #[error("Session timed out")]
    Timeout,
}

fn generate_session_id() -> String {
    use rand::Rng;
    let id: u64 = rand::thread_rng().r#gen();
    format!("{id:016X}")
}
