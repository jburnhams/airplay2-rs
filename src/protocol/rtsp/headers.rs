use std::collections::HashMap;

/// Well-known RTSP header names
pub mod names {
    pub const CSEQ: &str = "CSeq";
    pub const CONTENT_TYPE: &str = "Content-Type";
    pub const CONTENT_LENGTH: &str = "Content-Length";
    pub const SESSION: &str = "Session";
    pub const TRANSPORT: &str = "Transport";
    pub const USER_AGENT: &str = "User-Agent";
    pub const ACTIVE_REMOTE: &str = "Active-Remote";
    pub const DACP_ID: &str = "DACP-ID";
    pub const CLIENT_INSTANCE: &str = "Client-Instance";
    pub const X_APPLE_DEVICE_ID: &str = "X-Apple-Device-ID";
    pub const X_APPLE_SESSION_ID: &str = "X-Apple-Session-ID";
    pub const X_APPLE_PROTOCOL_VERSION: &str = "X-Apple-ProtocolVersion";
}

/// RAOP-specific header names
pub mod raop {
    /// Apple challenge for authentication
    pub const APPLE_CHALLENGE: &str = "Apple-Challenge";
    /// Apple response to challenge
    pub const APPLE_RESPONSE: &str = "Apple-Response";
    /// Audio latency in samples
    pub const AUDIO_LATENCY: &str = "Audio-Latency";
    /// Audio jack status
    pub const AUDIO_JACK_STATUS: &str = "Audio-Jack-Status";
    /// Client instance ID
    pub const CLIENT_INSTANCE: &str = "Client-Instance";
    /// DACP ID for remote control
    pub const DACP_ID: &str = "DACP-ID";
    /// Active remote token
    pub const ACTIVE_REMOTE: &str = "Active-Remote";
    /// Server info header
    pub const SERVER: &str = "Server";
    /// Range header for RECORD
    pub const RANGE: &str = "Range";
}

/// RTSP header collection
#[derive(Debug, Clone, Default)]
pub struct Headers {
    inner: HashMap<String, String>,
}

impl Headers {
    /// Create empty headers
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a header (case-insensitive key storage)
    ///
    /// If a header with the same name (case-insensitive) already exists, it is replaced.
    /// The new key casing is preserved.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name_str = name.into();
        // Remove existing key if any (case-insensitive)
        self.inner.retain(|k, _| !k.eq_ignore_ascii_case(&name_str));
        self.inner.insert(name_str, value.into());
    }

    /// Get header value (case-insensitive)
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        // RTSP headers are case-insensitive
        self.inner
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Check if header exists
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Get `CSeq` value
    #[must_use]
    pub fn cseq(&self) -> Option<u32> {
        self.get(names::CSEQ)?.parse().ok()
    }

    /// Get Content-Length value
    #[must_use]
    pub fn content_length(&self) -> Option<usize> {
        self.get(names::CONTENT_LENGTH)?.parse().ok()
    }

    /// Get Content-Type value
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.get(names::CONTENT_TYPE)
    }

    /// Get Session ID
    #[must_use]
    pub fn session(&self) -> Option<&str> {
        self.get(names::SESSION)
    }

    /// Iterate over all headers
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of headers
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl FromIterator<(String, String)> for Headers {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        let mut headers = Headers::new();
        for (k, v) in iter {
            headers.insert(k, v);
        }
        headers
    }
}
