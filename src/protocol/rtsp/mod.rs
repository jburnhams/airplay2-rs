//! Sans-IO RTSP protocol implementation for AirPlay

#![allow(unused_imports)]
#![allow(dead_code)]

pub mod request;
pub mod response;
pub mod codec;
pub mod session;
pub mod headers;

pub use request::{RtspRequest, RtspRequestBuilder};
pub use response::{RtspResponse, StatusCode};
pub use codec::{RtspCodec, RtspCodecError};
pub use session::{RtspSession, SessionState};
pub use headers::Headers;

/// RTSP methods used in AirPlay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Initiate session options negotiation
    Options,
    /// Announce stream information (SDP)
    Announce,
    /// Set up transport and session
    Setup,
    /// Start recording/streaming
    Record,
    /// Play (URL-based streaming)
    Play,
    /// Pause playback
    Pause,
    /// Flush buffers
    Flush,
    /// Tear down session
    Teardown,
    /// Set parameter (volume, progress, etc.)
    SetParameter,
    /// Get parameter (playback info, etc.)
    GetParameter,
    /// POST for pairing/auth
    Post,
}

impl Method {
    /// Convert to RTSP method string
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Options => "OPTIONS",
            Method::Announce => "ANNOUNCE",
            Method::Setup => "SETUP",
            Method::Record => "RECORD",
            Method::Play => "PLAY",
            Method::Pause => "PAUSE",
            Method::Flush => "FLUSH",
            Method::Teardown => "TEARDOWN",
            Method::SetParameter => "SET_PARAMETER",
            Method::GetParameter => "GET_PARAMETER",
            Method::Post => "POST",
        }
    }

    /// Parse from string
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "OPTIONS" => Some(Method::Options),
            "ANNOUNCE" => Some(Method::Announce),
            "SETUP" => Some(Method::Setup),
            "RECORD" => Some(Method::Record),
            "PLAY" => Some(Method::Play),
            "PAUSE" => Some(Method::Pause),
            "FLUSH" => Some(Method::Flush),
            "TEARDOWN" => Some(Method::Teardown),
            "SET_PARAMETER" => Some(Method::SetParameter),
            "GET_PARAMETER" => Some(Method::GetParameter),
            "POST" => Some(Method::Post),
            _ => None,
        }
    }
}
