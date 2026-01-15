use super::{Headers, RtspResponse, StatusCode};
use thiserror::Error;

/// Errors during RTSP parsing
#[derive(Debug, Error)]
pub enum RtspCodecError {
    #[error("incomplete data: need more bytes")]
    Incomplete,

    #[error("invalid status line: {0}")]
    InvalidStatusLine(String),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("invalid content length")]
    InvalidContentLength,

    #[error("response too large: {size} bytes")]
    ResponseTooLarge { size: usize },
}

/// Sans-IO RTSP codec for parsing responses
///
/// This codec handles incremental parsing of RTSP responses.
/// Feed bytes with `feed()`, check for complete responses with `decode()`.
pub struct RtspCodec {
    /// Internal buffer for partial data
    buffer: Vec<u8>,
    /// Maximum response size (default 1MB)
    max_size: usize,
    /// Parser state
    state: ParseState,
}

#[derive(Debug, Clone)]
enum ParseState {
    /// Waiting for status line
    StatusLine,
    /// Parsing headers
    Headers {
        version: String,
        status: StatusCode,
        reason: String,
    },
    /// Reading body
    Body {
        version: String,
        status: StatusCode,
        reason: String,
        headers: Headers,
        content_length: usize,
    },
}

impl RtspCodec {
    /// Create a new codec
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            max_size: 1024 * 1024, // 1MB
            state: ParseState::StatusLine,
        }
    }

    /// Set maximum response size
    #[must_use]
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }

    /// Feed bytes into the codec
    ///
    /// # Errors
    /// Returns `RtspCodecError::ResponseTooLarge` if the buffer exceeds `max_size`.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), RtspCodecError> {
        if self.buffer.len() + bytes.len() > self.max_size {
            return Err(RtspCodecError::ResponseTooLarge {
                size: self.buffer.len() + bytes.len(),
            });
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// Try to decode a complete response
    ///
    /// Returns `Ok(Some(response))` if a complete response is available,
    /// `Ok(None)` if more data is needed, or an error if parsing fails.
    ///
    /// # Errors
    /// Returns `RtspCodecError` if the response is invalid or too large.
    pub fn decode(&mut self) -> Result<Option<RtspResponse>, RtspCodecError> {
        loop {
            match &self.state {
                ParseState::StatusLine => {
                    if let Some(line_end) = self.find_line_end() {
                        let line = String::from_utf8_lossy(&self.buffer[..line_end]).to_string();
                        let (version, status, reason) = Self::parse_status_line(&line)?;

                        // Remove parsed line from buffer
                        self.buffer.drain(..line_end + 2);

                        self.state = ParseState::Headers {
                            version,
                            status,
                            reason,
                        };
                        continue;
                    }
                    return Ok(None);
                }

                ParseState::Headers {
                    version,
                    status,
                    reason,
                } => {
                    if let Some((headers, body_start)) = self.parse_headers()? {
                        let content_length = headers.content_length().unwrap_or(0);

                        // Remove headers from buffer
                        self.buffer.drain(..body_start);

                        if content_length == 0 {
                            // No body, response complete
                            let response = RtspResponse {
                                version: version.clone(),
                                status: *status,
                                reason: reason.clone(),
                                headers,
                                body: Vec::new(),
                            };
                            self.state = ParseState::StatusLine;
                            return Ok(Some(response));
                        }

                        self.state = ParseState::Body {
                            version: version.clone(),
                            status: *status,
                            reason: reason.clone(),
                            headers,
                            content_length,
                        };
                        continue;
                    }
                    return Ok(None);
                }

                ParseState::Body {
                    version,
                    status,
                    reason,
                    headers,
                    content_length,
                } => {
                    if self.buffer.len() >= *content_length {
                        let body = self.buffer.drain(..*content_length).collect();

                        let response = RtspResponse {
                            version: version.clone(),
                            status: *status,
                            reason: reason.clone(),
                            headers: headers.clone(),
                            body,
                        };

                        self.state = ParseState::StatusLine;
                        return Ok(Some(response));
                    }
                    return Ok(None);
                }
            }
        }
    }

    /// Clear the codec buffer and reset state
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state = ParseState::StatusLine;
    }

    /// Get current buffer length
    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    // Helper methods

    fn find_line_end(&self) -> Option<usize> {
        self.buffer.windows(2).position(|w| w == b"\r\n")
    }

    fn parse_status_line(line: &str) -> Result<(String, StatusCode, String), RtspCodecError> {
        // Format: "RTSP/1.0 200 OK"
        let mut parts = line.splitn(3, ' ');

        let version = parts
            .next()
            .ok_or_else(|| RtspCodecError::InvalidStatusLine(line.to_string()))?
            .to_string();

        let status = parts
            .next()
            .ok_or_else(|| RtspCodecError::InvalidStatusLine(line.to_string()))?
            .parse::<u16>()
            .map_err(|_| RtspCodecError::InvalidStatusLine(line.to_string()))?;

        let reason = parts.next().unwrap_or("").to_string();

        Ok((version, StatusCode(status), reason))
    }

    fn parse_headers(&self) -> Result<Option<(Headers, usize)>, RtspCodecError> {
        // Find end of headers (blank line)
        let header_end = self.buffer.windows(4).position(|w| w == b"\r\n\r\n");

        let Some(header_end) = header_end else {
            return Ok(None);
        };

        let header_bytes = &self.buffer[..header_end];
        let header_str = String::from_utf8_lossy(header_bytes);

        let mut headers = Headers::new();

        for line in header_str.split("\r\n") {
            if line.is_empty() {
                continue;
            }

            let colon_pos = line
                .find(':')
                .ok_or_else(|| RtspCodecError::InvalidHeader(line.to_string()))?;

            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();

            headers.insert(name, value);
        }

        // +4 for the \r\n\r\n
        Ok(Some((headers, header_end + 4)))
    }
}

impl Default for RtspCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_simple_response() {
        let mut codec = RtspCodec::new();

        codec
            .feed(
                b"RTSP/1.0 200 OK\r\n\
                     CSeq: 1\r\n\
                     \r\n",
            )
            .unwrap();

        let response = codec.decode().unwrap().unwrap();

        assert_eq!(response.version, "RTSP/1.0");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.reason, "OK");
        assert_eq!(response.cseq(), Some(1));
        assert!(response.body.is_empty());
    }

    #[test]
    fn test_decode_response_with_body() {
        let mut codec = RtspCodec::new();

        codec
            .feed(
                b"RTSP/1.0 200 OK\r\n\
                     CSeq: 2\r\n\
                     Content-Length: 5\r\n\
                     \r\n\
                     hello",
            )
            .unwrap();

        let response = codec.decode().unwrap().unwrap();

        assert_eq!(response.body, b"hello");
    }

    #[test]
    fn test_decode_incremental() {
        let mut codec = RtspCodec::new();

        // Feed partial data
        codec.feed(b"RTSP/1.0 200 ").unwrap();
        assert!(codec.decode().unwrap().is_none());

        codec.feed(b"OK\r\n").unwrap();
        assert!(codec.decode().unwrap().is_none());

        codec.feed(b"CSeq: 1\r\n\r\n").unwrap();
        assert!(codec.decode().unwrap().is_some());
    }

    #[test]
    fn test_decode_multiple_responses() {
        let mut codec = RtspCodec::new();

        codec
            .feed(
                b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n\
                     RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n",
            )
            .unwrap();

        let r1 = codec.decode().unwrap().unwrap();
        assert_eq!(r1.cseq(), Some(1));

        let r2 = codec.decode().unwrap().unwrap();
        assert_eq!(r2.cseq(), Some(2));

        assert!(codec.decode().unwrap().is_none());
    }

    #[test]
    fn test_decode_invalid_status_line() {
        let mut codec = RtspCodec::new();

        codec.feed(b"INVALID LINE\r\n\r\n").unwrap();

        let result = codec.decode();
        assert!(matches!(result, Err(RtspCodecError::InvalidStatusLine(_))));
    }

    #[test]
    fn test_status_code_checks() {
        assert!(StatusCode::OK.is_success());
        assert!(!StatusCode::OK.is_client_error());

        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(!StatusCode::NOT_FOUND.is_success());

        assert!(StatusCode::INTERNAL_ERROR.is_server_error());
    }

    #[test]
    fn test_max_size_limit() {
        let mut codec = RtspCodec::new().with_max_size(100);

        let result = codec.feed(&[0u8; 200]);

        assert!(matches!(
            result,
            Err(RtspCodecError::ResponseTooLarge { .. })
        ));
    }
}
