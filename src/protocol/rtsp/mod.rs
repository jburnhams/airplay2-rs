//! Sans-IO RTSP protocol implementation for `AirPlay`

#![allow(unused_imports)]
#![allow(dead_code)]

pub mod codec;
#[cfg(test)]
mod codec_tests;
#[cfg(test)]
mod compliance_tests;
#[cfg(test)]
mod header_parsing_tests;
pub mod headers;
#[cfg(test)]
mod headers_tests;
pub mod request;
#[cfg(test)]
mod request_tests;
pub mod response;
#[cfg(test)]
mod response_tests;
pub mod server_codec;
#[cfg(test)]
mod server_codec_tests;
pub mod session;
#[cfg(test)]
mod session_tests;
pub mod transport;
#[cfg(test)]
mod transport_tests;

pub use codec::{RtspCodec, RtspCodecError};
pub use headers::Headers;
pub use request::{RtspRequest, RtspRequestBuilder};
pub use response::{RtspResponse, StatusCode};
pub use session::{RtspSession, SessionState};

/// RTSP methods used in `AirPlay`
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
    /// GET for info
    Get,
    /// Set playback rate and anchor time
    SetRateAnchorTime,
}

impl Method {
    /// Convert to RTSP method string
    #[must_use]
    pub fn as_str(self) -> &'static str {
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
            Method::Get => "GET",
            Method::SetRateAnchorTime => "SETRATEANCHORTIME",
        }
    }
}

impl std::str::FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "OPTIONS" => Ok(Method::Options),
            "ANNOUNCE" => Ok(Method::Announce),
            "SETUP" => Ok(Method::Setup),
            "RECORD" => Ok(Method::Record),
            "PLAY" => Ok(Method::Play),
            "PAUSE" => Ok(Method::Pause),
            "FLUSH" => Ok(Method::Flush),
            "TEARDOWN" => Ok(Method::Teardown),
            "SET_PARAMETER" => Ok(Method::SetParameter),
            "GET_PARAMETER" => Ok(Method::GetParameter),
            "POST" => Ok(Method::Post),
            "GET" => Ok(Method::Get),
            "SETRATEANCHORTIME" => Ok(Method::SetRateAnchorTime),
            _ => Err(()),
        }
    }
}
