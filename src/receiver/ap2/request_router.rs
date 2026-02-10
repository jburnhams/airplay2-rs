//! Request routing for `AirPlay` 2 receiver
//!
//! `AirPlay` 2 uses both RTSP methods and HTTP-style POST endpoints.
//! This module classifies incoming requests and routes them appropriately.

use crate::protocol::rtsp::{Method, RtspRequest};

/// Classification of `AirPlay` 2 requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ap2RequestType {
    /// Standard RTSP request (OPTIONS, SETUP, RECORD, etc.)
    Rtsp(RtspMethod),

    /// HTTP-style endpoint (POST to specific paths)
    Endpoint(Ap2Endpoint),

    /// Unknown/unsupported request
    Unknown,
}

/// RTSP methods used in `AirPlay` 2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtspMethod {
    /// Initiate session options negotiation
    Options,
    /// Set up transport and session
    Setup,
    /// Start recording/streaming
    Record,
    /// Pause playback
    Pause,
    /// Flush buffers
    Flush,
    /// Tear down session
    Teardown,
    /// Get parameter (playback info, etc.)
    GetParameter,
    /// Set parameter (volume, progress, etc.)
    SetParameter,
    /// GET method (used for info endpoint mostly, but mapped to Endpoint there)
    Get,
}

/// HTTP-style endpoints in `AirPlay` 2
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ap2Endpoint {
    /// `GET /info` - device capabilities
    Info,

    /// `POST /pair-setup` - SRP pairing
    PairSetup,

    /// `POST /pair-verify` - session verification
    PairVerify,

    /// `POST /fp-setup` - `FairPlay` (not supported)
    FairPlaySetup,

    /// `POST /command` - playback commands
    Command,

    /// `POST /feedback` - status feedback
    Feedback,

    /// `POST /audioMode` - audio configuration
    AudioMode,

    /// `POST /auth-setup` - `MFi` authentication
    AuthSetup,

    /// Unknown endpoint
    Unknown(String),
}

impl Ap2RequestType {
    /// Classify an RTSP request
    #[must_use]
    pub fn classify(request: &RtspRequest) -> Self {
        match request.method {
            Method::Options => Self::Rtsp(RtspMethod::Options),
            Method::Setup => Self::Rtsp(RtspMethod::Setup),
            Method::Record => Self::Rtsp(RtspMethod::Record),
            Method::Pause => Self::Rtsp(RtspMethod::Pause),
            Method::Flush => Self::Rtsp(RtspMethod::Flush),
            Method::Teardown => Self::Rtsp(RtspMethod::Teardown),
            Method::GetParameter => Self::Rtsp(RtspMethod::GetParameter),
            Method::SetParameter => Self::Rtsp(RtspMethod::SetParameter),

            Method::Get => {
                // GET requests are routed by path
                Self::Endpoint(Self::classify_get_endpoint(&request.uri))
            }

            Method::Post => {
                // POST requests are routed by path
                Self::Endpoint(Self::classify_post_endpoint(&request.uri))
            }

            _ => Self::Unknown,
        }
    }

    fn classify_get_endpoint(uri: &str) -> Ap2Endpoint {
        // Extract path from URI (may be full URL or just path)
        let path = Self::extract_path(uri);

        match path {
            "/info" => Ap2Endpoint::Info,
            _ => Ap2Endpoint::Unknown(path.to_string()),
        }
    }

    fn classify_post_endpoint(uri: &str) -> Ap2Endpoint {
        let path = Self::extract_path(uri);

        match path {
            "/pair-setup" => Ap2Endpoint::PairSetup,
            "/pair-verify" => Ap2Endpoint::PairVerify,
            "/fp-setup" => Ap2Endpoint::FairPlaySetup,
            "/command" => Ap2Endpoint::Command,
            "/feedback" => Ap2Endpoint::Feedback,
            "/audioMode" => Ap2Endpoint::AudioMode,
            "/auth-setup" => Ap2Endpoint::AuthSetup,
            _ => Ap2Endpoint::Unknown(path.to_string()),
        }
    }

    fn extract_path(uri: &str) -> &str {
        // Handle both "rtsp://host/path" and "/path" formats
        if let Some(idx) = uri.find("://") {
            // Full URL: find path after host
            let after_scheme = &uri[idx + 3..];
            if let Some(path_idx) = after_scheme.find('/') {
                &after_scheme[path_idx..]
            } else {
                "/"
            }
        } else {
            // Just the path
            uri
        }
    }
}

impl Ap2Endpoint {
    /// Check if this endpoint requires authentication
    #[must_use]
    pub fn requires_auth(&self) -> bool {
        match self {
            // Pairing endpoints don't require prior auth
            Self::Info | Self::PairSetup | Self::PairVerify | Self::AuthSetup => false,

            // Everything else requires completed pairing
            Self::FairPlaySetup
            | Self::Command
            | Self::Feedback
            | Self::AudioMode
            | Self::Unknown(_) => true,
        }
    }

    /// Check if this endpoint accepts binary plist bodies
    #[must_use]
    pub fn expects_bplist(&self) -> bool {
        match self {
            Self::Info | Self::Unknown(_) => false,
            Self::PairSetup
            | Self::PairVerify
            | Self::FairPlaySetup
            | Self::Command
            | Self::Feedback
            | Self::AudioMode
            | Self::AuthSetup => true,
        }
    }
}
