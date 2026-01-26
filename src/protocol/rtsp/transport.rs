//! RTSP Transport header parsing
//!
//! The Transport header in SETUP requests specifies how audio will be delivered.
//! Format: `RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002`

/// Parsed Transport header
#[derive(Debug, Clone, PartialEq)]
pub struct TransportHeader {
    /// Protocol (always "RTP/AVP" for RAOP)
    pub protocol: String,
    /// Lower protocol (UDP or TCP)
    pub lower_transport: LowerTransport,
    /// Unicast or multicast
    pub cast: CastMode,
    /// Mode (usually "record" for RAOP)
    pub mode: Option<String>,
    /// Client's control port
    pub control_port: Option<u16>,
    /// Client's timing port
    pub timing_port: Option<u16>,
    /// Interleaved channel (for TCP transport)
    pub interleaved: Option<(u8, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerTransport {
    Udp,
    Tcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastMode {
    Unicast,
    Multicast,
}

impl TransportHeader {
    /// Parse a Transport header value
    ///
    /// # Errors
    /// Returns `TransportParseError` if the header string is invalid.
    pub fn parse(value: &str) -> Result<Self, TransportParseError> {
        let mut parts = value.split(';');

        // First part: protocol specification
        let proto_spec = parts.next().ok_or(TransportParseError::MissingProtocol)?;

        let (protocol, lower_transport) = Self::parse_protocol(proto_spec)?;

        let mut transport = TransportHeader {
            protocol,
            lower_transport,
            cast: CastMode::Unicast, // Default
            mode: None,
            control_port: None,
            timing_port: None,
            interleaved: None,
        };

        // Parse remaining parameters
        for part in parts {
            let part = part.trim();

            if part == "unicast" {
                transport.cast = CastMode::Unicast;
            } else if part == "multicast" {
                transport.cast = CastMode::Multicast;
            } else if let Some(value) = part.strip_prefix("mode=") {
                transport.mode = Some(value.to_string());
            } else if let Some(value) = part.strip_prefix("control_port=") {
                transport.control_port = Some(
                    value
                        .parse()
                        .map_err(|_| TransportParseError::InvalidPort)?,
                );
            } else if let Some(value) = part.strip_prefix("timing_port=") {
                transport.timing_port = Some(
                    value
                        .parse()
                        .map_err(|_| TransportParseError::InvalidPort)?,
                );
            } else if let Some(value) = part.strip_prefix("interleaved=") {
                transport.interleaved = Some(Self::parse_interleaved(value)?);
            }
            // Ignore unknown parameters
        }

        Ok(transport)
    }

    fn parse_protocol(spec: &str) -> Result<(String, LowerTransport), TransportParseError> {
        let parts: Vec<&str> = spec.split('/').collect();

        match parts.as_slice() {
            ["RTP", "AVP"] | ["RTP", "AVP", "UDP"] => {
                Ok(("RTP/AVP".to_string(), LowerTransport::Udp))
            }
            ["RTP", "AVP", "TCP"] => Ok(("RTP/AVP".to_string(), LowerTransport::Tcp)),
            _ => Err(TransportParseError::UnsupportedProtocol(spec.to_string())),
        }
    }

    fn parse_interleaved(value: &str) -> Result<(u8, u8), TransportParseError> {
        let parts: Vec<&str> = value.split('-').collect();
        match parts.as_slice() {
            [start, end] => {
                let start: u8 = start
                    .parse()
                    .map_err(|_| TransportParseError::InvalidPort)?;
                let end: u8 = end.parse().map_err(|_| TransportParseError::InvalidPort)?;
                Ok((start, end))
            }
            _ => Err(TransportParseError::InvalidInterleaved),
        }
    }

    /// Generate Transport header for response
    #[must_use]
    pub fn to_response_header(
        &self,
        server_port: u16,
        control_port: u16,
        timing_port: u16,
    ) -> String {
        let mut parts = vec![
            format!(
                "{}/{}",
                self.protocol,
                match self.lower_transport {
                    LowerTransport::Udp => "UDP",
                    LowerTransport::Tcp => "TCP",
                }
            ),
            self.cast.to_string(),
        ];

        if let Some(ref mode) = self.mode {
            parts.push(format!("mode={mode}"));
        }

        parts.push(format!("server_port={server_port}"));
        parts.push(format!("control_port={control_port}"));
        parts.push(format!("timing_port={timing_port}"));

        parts.join(";")
    }
}

impl std::fmt::Display for CastMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CastMode::Unicast => write!(f, "unicast"),
            CastMode::Multicast => write!(f, "multicast"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportParseError {
    #[error("Missing protocol specification")]
    MissingProtocol,

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Invalid port number")]
    InvalidPort,

    #[error("Invalid interleaved channel specification")]
    InvalidInterleaved,
}
