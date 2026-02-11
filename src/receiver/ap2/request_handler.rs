//! Unified request handler for `AirPlay` 2 receiver
//!
//! Routes requests to appropriate handlers based on classification,
//! manages session state, and handles encryption/decryption.

use super::request_router::{Ap2Endpoint, Ap2RequestType, RtspMethod};
use super::response_builder::Ap2ResponseBuilder;
use super::session_state::Ap2SessionState;
use crate::protocol::rtsp::{RtspRequest, StatusCode};

/// Result of handling a request
#[derive(Debug)]
pub struct Ap2HandleResult {
    /// Response bytes to send
    pub response: Vec<u8>,

    /// New session state (if changed)
    pub new_state: Option<Ap2SessionState>,

    /// Event to emit (for audio pipeline control)
    pub event: Option<Ap2Event>,

    /// Error that occurred (for logging)
    pub error: Option<String>,
}

/// Events emitted by request handling
#[derive(Debug, Clone)]
pub enum Ap2Event {
    /// Pairing completed, session keys available
    PairingComplete {
        /// Session key derived from pairing
        session_key: Vec<u8>,
    },

    /// First SETUP phase complete, timing/event channels ready
    SetupPhase1Complete {
        /// UDP port for timing synchronization
        timing_port: u16,
        /// UDP port for event channel
        event_port: u16,
    },

    /// Second SETUP phase complete, audio channels ready
    SetupPhase2Complete {
        /// UDP port for audio data
        audio_data_port: u16,
        /// UDP port for audio control
        audio_control_port: u16,
    },

    /// Streaming started
    StreamingStarted {
        /// Initial sequence number
        initial_sequence: u16,
        /// Initial RTP timestamp
        initial_timestamp: u32,
    },

    /// Streaming paused
    StreamingPaused,

    /// Buffer flush requested
    FlushRequested {
        /// Sequence number to flush until
        until_sequence: Option<u16>,
        /// Timestamp to flush until
        until_timestamp: Option<u32>,
    },

    /// Session teardown
    Teardown,

    /// Volume changed
    VolumeChanged {
        /// New volume level (dB)
        volume: f32,
    },

    /// Metadata updated
    MetadataUpdated,

    /// Command received
    CommandReceived {
        /// Command string
        command: String,
    },
}

/// Type alias for decryption function
pub type Decryptor = dyn Fn(&[u8]) -> Result<Vec<u8>, String>;

/// Context for request handling
pub struct Ap2RequestContext<'a> {
    /// Current session state
    pub state: &'a Ap2SessionState,

    /// Session ID (if established)
    pub session_id: Option<&'a str>,

    /// Encryption enabled
    pub encrypted: bool,

    /// Decryption function (if encrypted)
    pub decrypt: Option<&'a Decryptor>,
}

/// Handle an `AirPlay` 2 request
#[must_use]
pub fn handle_ap2_request(
    request: &RtspRequest,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    let cseq = request.headers.cseq().unwrap_or(0);
    let request_type = Ap2RequestType::classify(request);

    // Check if request is allowed in current state
    if !context.state.allows_method(request.method.as_str()) {
        return Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::METHOD_NOT_VALID)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(format!(
                "Method {} not allowed in state {:?}",
                request.method.as_str(),
                context.state
            )),
        };
    }

    // Check authentication requirements
    if let Ap2RequestType::Endpoint(ref endpoint) = request_type {
        if endpoint.requires_auth() && !context.state.is_authenticated() {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::auth_required(cseq).encode(),
                new_state: None,
                event: None,
                error: Some("Authentication required".to_string()),
            };
        }
    }

    // Route to appropriate handler
    match request_type {
        Ap2RequestType::Rtsp(method) => {
            handle_rtsp_method(method, request, cseq, context, handlers)
        }
        Ap2RequestType::Endpoint(endpoint) => {
            handle_endpoint(endpoint, request, cseq, context, handlers)
        }
        Ap2RequestType::Unknown => Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some("Unknown request type".to_string()),
        },
    }
}

fn handle_rtsp_method(
    method: RtspMethod,
    request: &RtspRequest,
    cseq: u32,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    match method {
        RtspMethod::Options => handle_options(cseq),
        RtspMethod::Setup => (handlers.setup)(request, cseq, context),
        RtspMethod::Record => (handlers.record)(request, cseq, context),
        RtspMethod::Pause => (handlers.pause)(request, cseq, context),
        RtspMethod::Flush => (handlers.flush)(request, cseq, context),
        RtspMethod::Teardown => (handlers.teardown)(request, cseq, context),
        RtspMethod::GetParameter => (handlers.get_parameter)(request, cseq, context),
        RtspMethod::SetParameter => (handlers.set_parameter)(request, cseq, context),
        RtspMethod::Get => {
            // GET /info is handled as endpoint
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_FOUND)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: None,
            }
        }
    }
}

fn handle_endpoint(
    endpoint: Ap2Endpoint,
    request: &RtspRequest,
    cseq: u32,
    context: &Ap2RequestContext,
    handlers: &Ap2Handlers,
) -> Ap2HandleResult {
    match endpoint {
        Ap2Endpoint::Info => (handlers.info)(request, cseq, context),
        Ap2Endpoint::PairSetup => (handlers.pair_setup)(request, cseq, context),
        Ap2Endpoint::PairVerify => (handlers.pair_verify)(request, cseq, context),
        Ap2Endpoint::Command => (handlers.command)(request, cseq, context),
        Ap2Endpoint::Feedback => (handlers.feedback)(request, cseq, context),
        Ap2Endpoint::AudioMode => (handlers.audio_mode)(request, cseq, context),
        Ap2Endpoint::AuthSetup => (handlers.auth_setup)(request, cseq, context),

        Ap2Endpoint::FairPlaySetup => {
            // FairPlay not supported
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some("FairPlay not supported".to_string()),
            }
        }

        Ap2Endpoint::Unknown(path) => {
            tracing::warn!("Unknown endpoint: {}", path);
            Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::NOT_FOUND)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some(format!("Unknown endpoint: {path}")),
            }
        }
    }
}

fn handle_options(cseq: u32) -> Ap2HandleResult {
    let methods = [
        "OPTIONS",
        "GET",
        "POST",
        "SETUP",
        "RECORD",
        "PAUSE",
        "FLUSH",
        "TEARDOWN",
        "GET_PARAMETER",
        "SET_PARAMETER",
    ]
    .join(", ");

    Ap2HandleResult {
        response: Ap2ResponseBuilder::ok()
            .cseq(cseq)
            .header("Public", &methods)
            .server("366.0")
            .encode(),
        new_state: None,
        event: None,
        error: None,
    }
}

/// Handler function type
pub type HandlerFn =
    Box<dyn Fn(&RtspRequest, u32, &Ap2RequestContext) -> Ap2HandleResult + Send + Sync>;

/// Collection of request handlers
pub struct Ap2Handlers {
    /// Handler for `/info` endpoint
    pub info: HandlerFn,
    /// Handler for `/pair-setup` endpoint
    pub pair_setup: HandlerFn,
    /// Handler for `/pair-verify` endpoint
    pub pair_verify: HandlerFn,
    /// Handler for `/auth-setup` endpoint
    pub auth_setup: HandlerFn,
    /// Handler for `SETUP` method
    pub setup: HandlerFn,
    /// Handler for `RECORD` method
    pub record: HandlerFn,
    /// Handler for `PAUSE` method
    pub pause: HandlerFn,
    /// Handler for `FLUSH` method
    pub flush: HandlerFn,
    /// Handler for `TEARDOWN` method
    pub teardown: HandlerFn,
    /// Handler for `GET_PARAMETER` method
    pub get_parameter: HandlerFn,
    /// Handler for `SET_PARAMETER` method
    pub set_parameter: HandlerFn,
    /// Handler for `/command` endpoint
    pub command: HandlerFn,
    /// Handler for `/feedback` endpoint
    pub feedback: HandlerFn,
    /// Handler for `/audioMode` endpoint
    pub audio_mode: HandlerFn,
}

impl Default for Ap2Handlers {
    fn default() -> Self {
        Self {
            info: Box::new(stub_handler),
            pair_setup: Box::new(stub_handler),
            pair_verify: Box::new(stub_handler),
            auth_setup: Box::new(stub_handler),
            setup: Box::new(stub_handler),
            record: Box::new(stub_handler),
            pause: Box::new(stub_handler),
            flush: Box::new(stub_handler),
            teardown: Box::new(stub_handler),
            get_parameter: Box::new(stub_handler),
            set_parameter: Box::new(stub_handler),
            command: Box::new(stub_handler),
            feedback: Box::new(stub_handler),
            audio_mode: Box::new(stub_handler),
        }
    }
}

/// Stub handler for unimplemented endpoints
fn stub_handler(
    _request: &RtspRequest,
    cseq: u32,
    _context: &Ap2RequestContext,
) -> Ap2HandleResult {
    Ap2HandleResult {
        response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
            .cseq(cseq)
            .encode(),
        new_state: None,
        event: None,
        error: Some("Handler not implemented".to_string()),
    }
}
