//! Server-side RTSP codec for parsing requests and generating responses
//!
//! This module complements the client-side codec by providing server-side
//! parsing. Both share the same request/response types but differ in what
//! they parse vs. generate.

use super::{Headers, Method, RtspRequest, RtspResponse, StatusCode};
use bytes::{Buf, BytesMut};
use std::str::{self, FromStr};

/// Errors during RTSP parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Incomplete data, need more bytes")]
    Incomplete,

    #[error("Invalid request line: {0}")]
    InvalidRequestLine(String),

    #[error("Invalid method: {0}")]
    InvalidMethod(String),

    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    #[error("Invalid Content-Length: {0}")]
    InvalidContentLength(String),

    #[error("Body too large: {size} > {max}")]
    BodyTooLarge { size: usize, max: usize },

    #[error("Invalid UTF-8 in headers")]
    InvalidUtf8,
}

/// Maximum allowed body size (16 MB should be plenty for any RTSP body)
const MAX_BODY_SIZE: usize = 16 * 1024 * 1024;

/// Maximum header section size (64 KB)
const MAX_HEADER_SIZE: usize = 64 * 1024;

/// Server-side RTSP codec
///
/// Parses incoming RTSP requests from a byte buffer and generates
/// properly formatted RTSP responses.
///
/// # Sans-IO Design
///
/// This codec performs no I/O. It operates on byte buffers:
/// - `feed()` adds bytes to the internal buffer
/// - `decode()` attempts to parse a complete request
/// - `encode_response()` generates response bytes
///
/// # Example
///
/// ```rust
/// use airplay2::protocol::rtsp::server_codec::RtspServerCodec;
///
/// let mut codec = RtspServerCodec::new();
///
/// // Feed incoming bytes
/// codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
///
/// // Try to decode
/// if let Some(request) = codec.decode().unwrap() {
///     println!("Method: {:?}", request.method);
/// }
/// ```
pub struct RtspServerCodec {
    buffer: BytesMut,
}

impl RtspServerCodec {
    /// Create a new server codec
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Feed bytes into the internal buffer
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Get current buffer length
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Attempt to decode a complete RTSP request
    ///
    /// Returns:
    /// - `Ok(Some(request))` if a complete request was parsed
    /// - `Ok(None)` if more data is needed
    /// - `Err(e)` if parsing failed
    ///
    /// # Errors
    /// Returns `ParseError` if the request is malformed.
    pub fn decode(&mut self) -> Result<Option<RtspRequest>, ParseError> {
        // Find header/body separator
        let Some(header_end) = self.find_header_end() else {
            // Check for header overflow
            if self.buffer.len() > MAX_HEADER_SIZE {
                return Err(ParseError::InvalidHeader("Headers too large".into()));
            }
            return Ok(None); // Need more data
        };

        // Parse headers (without consuming buffer yet)
        let header_bytes = &self.buffer[..header_end];
        let header_str = str::from_utf8(header_bytes).map_err(|_| ParseError::InvalidUtf8)?;

        let (method, uri, headers) = Self::parse_headers(header_str)?;

        // Determine body length
        let content_length = headers
            .get("Content-Length")
            .or_else(|| headers.get("content-length"))
            .map(str::parse::<usize>)
            .transpose()
            .map_err(|_| ParseError::InvalidContentLength("Not a number".into()))?
            .unwrap_or(0);

        if content_length > MAX_BODY_SIZE {
            return Err(ParseError::BodyTooLarge {
                size: content_length,
                max: MAX_BODY_SIZE,
            });
        }

        // Total message size: headers + \r\n\r\n + body
        let total_size = header_end + 4 + content_length;

        if self.buffer.len() < total_size {
            return Ok(None); // Need more data for body
        }

        // Now consume the buffer
        let _ = self.buffer.split_to(header_end + 4); // Headers + separator
        let body = if content_length > 0 {
            self.buffer.split_to(content_length).to_vec()
        } else {
            Vec::new()
        };

        Ok(Some(RtspRequest {
            method,
            uri,
            headers,
            body,
        }))
    }

    /// Find the position of header/body separator (\r\n\r\n)
    fn find_header_end(&self) -> Option<usize> {
        let needle = b"\r\n\r\n";
        self.buffer
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// Parse request line and headers
    fn parse_headers(header_str: &str) -> Result<(Method, String, Headers), ParseError> {
        let mut lines = header_str.lines();

        // Parse request line: "METHOD uri RTSP/1.0"
        let request_line = lines
            .next()
            .ok_or_else(|| ParseError::InvalidRequestLine("Empty request".into()))?;

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(ParseError::InvalidRequestLine(request_line.to_string()));
        }

        let method = Method::from_str(parts[0])
            .map_err(|()| ParseError::InvalidMethod(parts[0].to_string()))?;
        let uri = parts[1].to_string();

        // Validate protocol version
        if !parts[2].starts_with("RTSP/") {
            return Err(ParseError::InvalidRequestLine(format!(
                "Invalid protocol: {}",
                parts[2]
            )));
        }

        // Parse headers
        let mut headers = Headers::new();
        for line in lines {
            if line.is_empty() {
                break;
            }

            if let Some(pos) = line.find(':') {
                let name = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                headers.insert(name, value);
            } else {
                return Err(ParseError::InvalidHeader(line.to_string()));
            }
        }

        Ok((method, uri, headers))
    }
}

impl Default for RtspServerCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for RTSP responses
///
/// Provides a fluent interface for constructing RTSP responses with
/// proper formatting for the wire protocol.
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status: StatusCode,
    headers: Headers,
    body: Option<Vec<u8>>,
}

impl ResponseBuilder {
    /// Create a new response builder with the given status
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: None,
        }
    }

    /// Create an OK (200) response
    #[must_use]
    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    /// Create an error response
    #[must_use]
    pub fn error(status: StatusCode) -> Self {
        Self::new(status)
    }

    /// Set the `CSeq` header (required - should match request)
    #[must_use]
    pub fn cseq(mut self, cseq: u32) -> Self {
        self.headers.insert("CSeq".to_string(), cseq.to_string());
        self
    }

    /// Set the Session header
    #[must_use]
    pub fn session(mut self, session_id: &str) -> Self {
        self.headers
            .insert("Session".to_string(), session_id.to_string());
        self
    }

    /// Add a custom header
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Set a text body (will set Content-Type to text/parameters)
    #[must_use]
    pub fn text_body(mut self, body: &str) -> Self {
        self.body = Some(body.as_bytes().to_vec());
        self.headers
            .insert("Content-Type".to_string(), "text/parameters".to_string());
        self
    }

    /// Set a binary body
    #[must_use]
    pub fn binary_body(mut self, body: Vec<u8>, content_type: &str) -> Self {
        self.body = Some(body);
        self.headers
            .insert("Content-Type".to_string(), content_type.to_string());
        self
    }

    /// Set SDP body (for ANNOUNCE responses if needed)
    #[must_use]
    pub fn sdp_body(mut self, sdp: &str) -> Self {
        self.body = Some(sdp.as_bytes().to_vec());
        self.headers
            .insert("Content-Type".to_string(), "application/sdp".to_string());
        self
    }

    /// Set the Audio-Latency header (used in RECORD response)
    #[must_use]
    pub fn audio_latency(mut self, samples: u32) -> Self {
        self.headers
            .insert("Audio-Latency".to_string(), samples.to_string());
        self
    }

    /// Build into an `RtspResponse`
    #[must_use]
    pub fn build(mut self) -> RtspResponse {
        // Add Content-Length if body present
        if let Some(ref body) = self.body {
            self.headers
                .insert("Content-Length".to_string(), body.len().to_string());
        }

        RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: self.status,
            reason: status_reason(self.status).to_string(),
            headers: self.headers,
            body: self.body.unwrap_or_default(),
        }
    }

    /// Encode directly to bytes
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let response = self.build();
        encode_response(&response)
    }
}

/// Encode an RTSP response to bytes
#[must_use]
pub fn encode_response(response: &RtspResponse) -> Vec<u8> {
    let mut output = Vec::with_capacity(256 + response.body.len());

    // Status line
    output.extend_from_slice(
        format!(
            "{} {} {}\r\n",
            response.version,
            response.status.as_u16(),
            response.reason
        )
        .as_bytes(),
    );

    // Headers
    for (name, value) in response.headers.iter() {
        output.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }

    // Separator
    output.extend_from_slice(b"\r\n");

    // Body
    if !response.body.is_empty() {
        output.extend_from_slice(&response.body);
    }

    output
}

/// Get reason phrase for status code
fn status_reason(status: StatusCode) -> &'static str {
    match status.as_u16() {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        451 => "Parameter Not Understood",
        453 => "Not Enough Bandwidth",
        454 => "Session Not Found",
        455 => "Method Not Valid in This State",
        457 => "Invalid Range",
        459 => "Aggregate Operation Not Allowed",
        460 => "Only Aggregate Operation Allowed",
        461 => "Unsupported Transport",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}
