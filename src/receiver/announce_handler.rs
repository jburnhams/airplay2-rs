//! ANNOUNCE request handler
//!
//! Processes ANNOUNCE requests and configures session stream parameters.

use crate::protocol::rtsp::RtspRequest;
use crate::protocol::sdp::{raop::extract_stream_parameters, SdpParseError, SdpParser};
use crate::receiver::session::{ReceiverSession, StreamParameters};

/// Errors from ANNOUNCE handling
#[derive(Debug, thiserror::Error)]
pub enum AnnounceError {
    /// Empty body in ANNOUNCE
    #[error("Empty body in ANNOUNCE")]
    EmptyBody,

    /// Body is not valid UTF-8
    #[error("Body is not valid UTF-8")]
    InvalidUtf8,

    /// SDP parse error
    #[error("SDP parse error: {0}")]
    SdpParse(#[from] SdpParseError),

    /// Unsupported codec
    #[error("Unsupported codec")]
    UnsupportedCodec,
}

/// Process an ANNOUNCE request
///
/// # Errors
/// Returns `AnnounceError` if the request body is invalid or SDP parsing fails.
pub fn process_announce(
    request: &RtspRequest,
    rsa_private_key: Option<&[u8]>,
) -> Result<StreamParameters, AnnounceError> {
    if request.body.is_empty() {
        return Err(AnnounceError::EmptyBody);
    }

    let sdp_str = std::str::from_utf8(&request.body).map_err(|_| AnnounceError::InvalidUtf8)?;

    let sdp = SdpParser::parse(sdp_str)?;

    let params = extract_stream_parameters(&sdp, rsa_private_key)?;

    Ok(params)
}

/// Apply stream parameters to session
pub fn apply_to_session(session: &mut ReceiverSession, params: StreamParameters) {
    session.set_stream_params(params);
}
