use super::{Headers, Method, headers::names};

/// An RTSP request message
#[derive(Debug, Clone)]
pub struct RtspRequest {
    /// HTTP method
    pub method: Method,
    /// Request URI (e.g., "rtsp://192.168.1.10/1234567")
    pub uri: String,
    /// Request headers
    pub headers: Headers,
    /// Request body (may be empty)
    pub body: Vec<u8>,
}

impl RtspRequest {
    /// Create a new request
    pub fn new(method: Method, uri: impl Into<String>) -> Self {
        Self {
            method,
            uri: uri.into(),
            headers: Headers::new(),
            body: Vec::new(),
        }
    }

    /// Create a builder for constructing requests
    pub fn builder(method: Method, uri: impl Into<String>) -> RtspRequestBuilder {
        RtspRequestBuilder::new(method, uri)
    }

    /// Encode request to bytes
    ///
    /// Returns the complete RTSP request ready for transmission
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(256 + self.body.len());

        // Request line: METHOD uri RTSP/1.0\r\n
        output.extend_from_slice(self.method.as_str().as_bytes());
        output.push(b' ');
        output.extend_from_slice(self.uri.as_bytes());
        output.extend_from_slice(b" RTSP/1.0\r\n");

        // Headers
        for (name, value) in self.headers.iter() {
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }

        // Content-Length if body present
        if !self.body.is_empty() {
            let len_header = format!("{}: {}\r\n", names::CONTENT_LENGTH, self.body.len());
            output.extend_from_slice(len_header.as_bytes());
        }

        // End of headers
        output.extend_from_slice(b"\r\n");

        // Body
        output.extend_from_slice(&self.body);

        output
    }
}

/// Builder for RTSP requests
#[derive(Debug)]
pub struct RtspRequestBuilder {
    request: RtspRequest,
}

impl RtspRequestBuilder {
    /// Create a new builder
    pub fn new(method: Method, uri: impl Into<String>) -> Self {
        Self {
            request: RtspRequest::new(method, uri),
        }
    }

    /// Add a header
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.request.headers.insert(name, value);
        self
    }

    /// Set CSeq header
    #[must_use]
    pub fn cseq(self, seq: u32) -> Self {
        self.header(names::CSEQ, seq.to_string())
    }

    /// Set Content-Type header
    #[must_use]
    pub fn content_type(self, content_type: &str) -> Self {
        self.header(names::CONTENT_TYPE, content_type)
    }

    /// Set User-Agent header
    #[must_use]
    pub fn user_agent(self, agent: &str) -> Self {
        self.header(names::USER_AGENT, agent)
    }

    /// Set session ID header
    #[must_use]
    pub fn session(self, session_id: &str) -> Self {
        self.header(names::SESSION, session_id)
    }

    /// Set body as raw bytes
    #[must_use]
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.request.body = body;
        self
    }

    /// Set body as binary plist
    #[must_use]
    pub fn body_plist(mut self, plist: &crate::protocol::plist::PlistValue) -> Self {
        self.request.body = crate::protocol::plist::encode(plist)
            .expect("plist encoding should not fail");
        self.request.headers.insert(
            names::CONTENT_TYPE,
            "application/x-apple-binary-plist".to_string(),
        );
        self
    }

    /// Build the request
    #[must_use]
    pub fn build(self) -> RtspRequest {
        self.request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_encode_simple() {
        let request = RtspRequest::builder(Method::Options, "rtsp://192.168.1.10:7000/*")
            .cseq(1)
            .user_agent("test/1.0")
            .build();

        let encoded = request.encode();
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(encoded_str.starts_with("OPTIONS rtsp://192.168.1.10:7000/* RTSP/1.0\r\n"));
        assert!(encoded_str.contains("CSeq: 1\r\n"));
        assert!(encoded_str.contains("User-Agent: test/1.0\r\n"));
        assert!(encoded_str.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_request_encode_with_body() {
        let body = b"test body content".to_vec();
        let request = RtspRequest::builder(Method::SetParameter, "rtsp://example.com/")
            .cseq(5)
            .content_type("text/parameters")
            .body(body.clone())
            .build();

        let encoded = request.encode();
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(encoded_str.contains("Content-Type: text/parameters\r\n"));
        assert!(encoded_str.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(encoded_str.ends_with("test body content"));
    }

    #[test]
    fn test_method_as_str() {
        assert_eq!(Method::Options.as_str(), "OPTIONS");
        assert_eq!(Method::Setup.as_str(), "SETUP");
        assert_eq!(Method::SetParameter.as_str(), "SET_PARAMETER");
    }

    #[test]
    fn test_method_from_str() {
        assert_eq!(Method::from_str("OPTIONS"), Some(Method::Options));
        assert_eq!(Method::from_str("options"), Some(Method::Options));
        assert_eq!(Method::from_str("INVALID"), None);
    }
}
