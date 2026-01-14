use super::Headers;

/// RTSP status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(pub u16);

impl StatusCode {
    pub const OK: StatusCode = StatusCode(200);
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    pub const NOT_ACCEPTABLE: StatusCode = StatusCode(406);
    pub const INTERNAL_ERROR: StatusCode = StatusCode(500);
    pub const NOT_IMPLEMENTED: StatusCode = StatusCode(501);
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);

    /// Check if this is a success status (2xx)
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// Check if this is a client error (4xx)
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// Check if this is a server error (5xx)
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// Get status code as u16
    #[must_use]
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

/// An RTSP response message
#[derive(Debug, Clone)]
pub struct RtspResponse {
    /// RTSP version (usually "RTSP/1.0")
    pub version: String,
    /// Status code
    pub status: StatusCode,
    /// Reason phrase (e.g., "OK")
    pub reason: String,
    /// Response headers
    pub headers: Headers,
    /// Response body (may be empty)
    pub body: Vec<u8>,
}

impl RtspResponse {
    /// Check if response indicates success
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Get CSeq from response
    #[must_use]
    pub fn cseq(&self) -> Option<u32> {
        self.headers.cseq()
    }

    /// Get session ID from response
    #[must_use]
    pub fn session(&self) -> Option<&str> {
        self.headers.session()
    }

    /// Parse body as binary plist
    ///
    /// # Errors
    /// Returns `PlistDecodeError` if the body cannot be decoded as a binary plist.
    pub fn body_as_plist(&self) -> Result<crate::protocol::plist::PlistValue, crate::protocol::plist::PlistDecodeError> {
        crate::protocol::plist::decode(&self.body)
    }

    /// Check if body is binary plist
    #[must_use]
    pub fn is_plist(&self) -> bool {
        self.headers
            .content_type()
            .map_or(false, |ct| ct.contains("apple-binary-plist") || ct.contains("application/x-plist"))
    }
}
