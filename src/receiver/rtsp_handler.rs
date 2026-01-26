//! RTSP request handlers for the receiver
//!
//! This module provides the logic for handling each RTSP method.
//! Handlers are pure functions that take a request and session state,
//! returning a response. No I/O is performed.

use crate::protocol::rtsp::{
    Method, RtspRequest, RtspResponse, StatusCode, server_codec::ResponseBuilder,
    transport::TransportHeader,
};
use crate::receiver::session::{ReceiverSession, SessionState};

/// Result of handling an RTSP request
#[derive(Debug)]
pub struct HandleResult {
    /// Response to send back
    pub response: RtspResponse,
    /// New session state (if changed)
    pub new_state: Option<SessionState>,
    /// Allocated ports (for SETUP)
    pub allocated_ports: Option<AllocatedPorts>,
    /// Should start streaming (for RECORD)
    pub start_streaming: bool,
    /// Should stop streaming (for TEARDOWN)
    pub stop_streaming: bool,
}

/// Ports allocated during SETUP
#[derive(Debug, Clone, Copy)]
pub struct AllocatedPorts {
    /// UDP port for audio stream
    pub audio_port: u16,
    /// UDP port for control stream
    pub control_port: u16,
    /// UDP port for timing/sync
    pub timing_port: u16,
}

/// Handle an incoming RTSP request
#[must_use]
pub fn handle_request(request: &RtspRequest, session: &ReceiverSession) -> HandleResult {
    let cseq = request.headers.cseq().unwrap_or(0);

    match request.method {
        Method::Options => handle_options(cseq),
        Method::Announce => handle_announce(request, cseq, session),
        Method::Setup => handle_setup(request, cseq, session),
        Method::Record => handle_record(request, cseq, session),
        Method::Pause => handle_pause(cseq, session),
        Method::Flush => handle_flush(request, cseq),
        Method::Teardown => handle_teardown(cseq, session),
        Method::GetParameter => handle_get_parameter(request, cseq, session),
        Method::SetParameter => handle_set_parameter(request, cseq, session),
        Method::Post => handle_post(request, cseq, session),
        _ => handle_unknown(cseq),
    }
}

/// Handle OPTIONS request
fn handle_options(cseq: u32) -> HandleResult {
    let methods = [
        "ANNOUNCE",
        "SETUP",
        "RECORD",
        "PAUSE",
        "FLUSH",
        "TEARDOWN",
        "OPTIONS",
        "GET_PARAMETER",
        "SET_PARAMETER",
        "POST",
    ]
    .join(", ");

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .header("Public", &methods)
        .build();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle ANNOUNCE request (SDP body with stream parameters)
fn handle_announce(_request: &RtspRequest, cseq: u32, session: &ReceiverSession) -> HandleResult {
    // Verify state
    if session.state() != SessionState::Connected {
        return error_result(StatusCode::METHOD_NOT_VALID, cseq);
    }

    // SDP parsing is handled by Section 38
    // Here we just acknowledge receipt

    let response = ResponseBuilder::ok().cseq(cseq).build();

    HandleResult {
        response,
        new_state: Some(SessionState::Announced),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle SETUP request
fn handle_setup(request: &RtspRequest, cseq: u32, _session: &ReceiverSession) -> HandleResult {
    // Parse Transport header
    let Some(transport_str) = request.headers.get("Transport") else {
        return error_result(StatusCode::BAD_REQUEST, cseq);
    };

    let Ok(client_transport) = TransportHeader::parse(transport_str) else {
        return error_result(StatusCode::BAD_REQUEST, cseq);
    };

    // Ports will be allocated by the session manager
    // Here we return placeholder that will be filled in by caller
    let ports = AllocatedPorts {
        audio_port: 0, // Placeholder
        control_port: 0,
        timing_port: 0,
    };

    // Generate session ID
    let session_id = generate_session_id();

    // Build response Transport header
    let response_transport = client_transport.to_response_header(
        ports.audio_port,
        ports.control_port,
        ports.timing_port,
    );

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .session(&session_id)
        .header("Transport", &response_transport)
        .build();

    HandleResult {
        response,
        new_state: Some(SessionState::Setup),
        allocated_ports: Some(ports),
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle RECORD request (start streaming)
fn handle_record(request: &RtspRequest, cseq: u32, session: &ReceiverSession) -> HandleResult {
    if session.state() != SessionState::Setup {
        return error_result(StatusCode::METHOD_NOT_VALID, cseq);
    }

    // Parse RTP-Info header for initial sequence/timestamp
    // Format: seq=<seq>;rtptime=<timestamp>
    let _rtp_info = request.headers.get("RTP-Info");

    // Report our audio latency (in samples at 44.1kHz)
    // 2 seconds = 88200 samples
    let latency_samples: u32 = 88200;

    let response = ResponseBuilder::ok()
        .cseq(cseq)
        .audio_latency(latency_samples)
        .build();

    HandleResult {
        response,
        new_state: Some(SessionState::Streaming),
        allocated_ports: None,
        start_streaming: true,
        stop_streaming: false,
    }
}

/// Handle PAUSE request
fn handle_pause(cseq: u32, _session: &ReceiverSession) -> HandleResult {
    let response = ResponseBuilder::ok().cseq(cseq).build();

    HandleResult {
        response,
        new_state: Some(SessionState::Paused),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false, // Keep session alive, just pause output
    }
}

/// Handle FLUSH request (clear buffer)
fn handle_flush(request: &RtspRequest, cseq: u32) -> HandleResult {
    // Parse RTP-Info for flush point
    // Format: rtptime=<timestamp>
    let _rtp_info = request.headers.get("RTP-Info");

    let response = ResponseBuilder::ok().cseq(cseq).build();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle TEARDOWN request
fn handle_teardown(cseq: u32, _session: &ReceiverSession) -> HandleResult {
    let response = ResponseBuilder::ok().cseq(cseq).build();

    HandleResult {
        response,
        new_state: Some(SessionState::Teardown),
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: true,
    }
}

/// Handle `GET_PARAMETER` (keep-alive, status queries)
fn handle_get_parameter(
    request: &RtspRequest,
    cseq: u32,
    session: &ReceiverSession,
) -> HandleResult {
    // Body may contain parameter names to query
    // Empty body = keep-alive ping

    let body_str = String::from_utf8_lossy(&request.body);

    let response_body = if body_str.contains("volume") {
        format!("volume: {:.6}\r\n", session.volume())
    } else {
        String::new()
    };

    let response = if response_body.is_empty() {
        ResponseBuilder::ok().cseq(cseq).build()
    } else {
        ResponseBuilder::ok()
            .cseq(cseq)
            .text_body(&response_body)
            .build()
    };

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle `SET_PARAMETER` (volume, metadata, etc.)
fn handle_set_parameter(
    request: &RtspRequest,
    cseq: u32,
    _session: &ReceiverSession,
) -> HandleResult {
    // Content-Type determines what's being set
    let _content_type = request.headers.get("Content-Type").unwrap_or("");

    // Delegate to appropriate handler based on content type
    // Section 43 handles the detailed parsing

    let response = ResponseBuilder::ok().cseq(cseq).build();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle POST (pairing, auth)
fn handle_post(_request: &RtspRequest, cseq: u32, _session: &ReceiverSession) -> HandleResult {
    // POST is used for pairing endpoints like /pair-setup, /pair-verify
    // For now, return not implemented

    let response = ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
        .cseq(cseq)
        .build();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Handle unknown method
fn handle_unknown(cseq: u32) -> HandleResult {
    error_result(StatusCode::METHOD_NOT_ALLOWED, cseq)
}

/// Generate an error result
fn error_result(status: StatusCode, cseq: u32) -> HandleResult {
    let response = ResponseBuilder::error(status).cseq(cseq).build();

    HandleResult {
        response,
        new_state: None,
        allocated_ports: None,
        start_streaming: false,
        stop_streaming: false,
    }
}

/// Generate a random session ID
fn generate_session_id() -> String {
    use rand::Rng;
    let id: u64 = rand::thread_rng().r#gen();
    format!("{id:016X}")
}
