//! Extended RTSP response builder for `AirPlay` 2
//!
//! Adds support for binary plist bodies and `AirPlay` 2-specific headers.

use super::body_handler::{content_types, encode_bplist_body};
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::StatusCode;
use crate::protocol::rtsp::server_codec::ResponseBuilder;

/// Extended response builder for `AirPlay` 2
pub struct Ap2ResponseBuilder {
    inner: ResponseBuilder,
}

impl Ap2ResponseBuilder {
    /// Create a new OK response
    #[must_use]
    pub fn ok() -> Self {
        Self {
            inner: ResponseBuilder::ok(),
        }
    }

    /// Create an error response
    #[must_use]
    pub fn error(status: StatusCode) -> Self {
        Self {
            inner: ResponseBuilder::error(status),
        }
    }

    /// Set the `CSeq` header
    #[must_use]
    pub fn cseq(mut self, cseq: u32) -> Self {
        self.inner = self.inner.cseq(cseq);
        self
    }

    /// Set the Session header
    #[must_use]
    pub fn session(mut self, session_id: &str) -> Self {
        self.inner = self.inner.session(session_id);
        self
    }

    /// Add a custom header
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.inner = self.inner.header(name, value);
        self
    }

    /// Set binary plist body
    ///
    /// # Errors
    ///
    /// Returns `Ap2ResponseError` if the body encoding fails.
    pub fn bplist_body(self, value: &PlistValue) -> Result<Self, Ap2ResponseError> {
        let body = encode_bplist_body(value).map_err(Ap2ResponseError::EncodeError)?;

        Ok(Self {
            inner: self.inner.binary_body(body, content_types::BINARY_PLIST),
        })
    }

    /// Set raw binary body with octet-stream content type
    #[must_use]
    pub fn binary_body(self, body: Vec<u8>) -> Self {
        Self {
            inner: self.inner.binary_body(body, content_types::OCTET_STREAM),
        }
    }

    /// Set text/parameters body
    #[must_use]
    pub fn text_body(mut self, body: &str) -> Self {
        self.inner = self.inner.text_body(body);
        self
    }

    /// Add Server header (common for `AirPlay` 2)
    #[must_use]
    pub fn server(self, version: &str) -> Self {
        self.header("Server", &format!("AirTunes/{version}"))
    }

    /// Add timing headers for SETUP response
    #[must_use]
    pub fn timing_port(self, port: u16) -> Self {
        self.header("Timing-Port", &port.to_string())
    }

    /// Add event port header for SETUP response
    #[must_use]
    pub fn event_port(self, port: u16) -> Self {
        self.header("Event-Port", &port.to_string())
    }

    /// Encode to bytes
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        self.inner.encode()
    }
}

/// Common response helpers
impl Ap2ResponseBuilder {
    /// Create response for successful pairing step
    #[must_use]
    pub fn pairing_response(cseq: u32, body: Vec<u8>) -> Self {
        Self::ok().cseq(cseq).binary_body(body)
    }

    /// Create response for authentication required
    #[must_use]
    pub fn auth_required(cseq: u32) -> Self {
        Self::error(StatusCode(470)) // Connection Authorization Required
            .cseq(cseq)
    }

    /// Create response for bad request with error dict
    ///
    /// # Errors
    ///
    /// Returns `Ap2ResponseError` if the error body encoding fails.
    pub fn bad_request_with_error(
        cseq: u32,
        code: i64,
        message: &str,
    ) -> Result<Self, Ap2ResponseError> {
        use std::collections::HashMap;

        let mut error_dict = HashMap::new();
        error_dict.insert("code".to_string(), PlistValue::Integer(code));
        error_dict.insert(
            "message".to_string(),
            PlistValue::String(message.to_string()),
        );

        let plist = PlistValue::Dictionary(error_dict);

        Self::error(StatusCode::BAD_REQUEST)
            .cseq(cseq)
            .bplist_body(&plist)
    }
}

/// Errors occurring during response generation
#[derive(Debug, thiserror::Error)]
pub enum Ap2ResponseError {
    /// Error encoding the response body
    #[error("Failed to encode body: {0}")]
    EncodeError(#[from] super::body_handler::BodyParseError),
}
