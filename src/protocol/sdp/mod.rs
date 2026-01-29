//! SDP (Session Description Protocol) for RAOP
//!
//! RAOP uses SDP in the ANNOUNCE request to describe the audio stream.

mod builder;
mod parser;
pub mod raop;

#[cfg(test)]
mod raop_tests;
#[cfg(test)]
mod tests;

pub use builder::{SdpBuilder, create_raop_announce_sdp};
pub use parser::{SdpParseError, SdpParser};

use std::collections::HashMap;

/// SDP session description
#[derive(Debug, Clone, Default)]
pub struct SessionDescription {
    /// Protocol version (v=)
    pub version: u8,
    /// Origin (o=)
    pub origin: Option<SdpOrigin>,
    /// Session name (s=)
    pub session_name: String,
    /// Connection info (c=)
    pub connection: Option<SdpConnection>,
    /// Timing (t=)
    pub timing: Option<(u64, u64)>,
    /// Media descriptions (m=)
    pub media: Vec<MediaDescription>,
    /// Session-level attributes (a=)
    pub attributes: HashMap<String, Option<String>>,
}

/// SDP origin field (o=)
#[derive(Debug, Clone)]
pub struct SdpOrigin {
    /// Username
    pub username: String,
    /// Session ID
    pub session_id: String,
    /// Session version
    pub session_version: String,
    /// Network type (usually "IN")
    pub net_type: String,
    /// Address type (usually "IP4" or "IP6")
    pub addr_type: String,
    /// Unicast address
    pub unicast_address: String,
}

/// SDP connection field (c=)
#[derive(Debug, Clone)]
pub struct SdpConnection {
    /// Network type
    pub net_type: String,
    /// Address type
    pub addr_type: String,
    /// Connection address
    pub address: String,
}

/// SDP media description (m=)
#[derive(Debug, Clone)]
pub struct MediaDescription {
    /// Media type (audio, video, etc.)
    pub media_type: String,
    /// Port number
    pub port: u16,
    /// Protocol (RTP/AVP, etc.)
    pub protocol: String,
    /// Format list (payload types)
    pub formats: Vec<String>,
    /// Media-level attributes
    pub attributes: HashMap<String, Option<String>>,
}

impl SessionDescription {
    /// Get a session-level attribute
    #[must_use]
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name)?.as_deref()
    }

    /// Get the audio media description
    #[must_use]
    pub fn audio_media(&self) -> Option<&MediaDescription> {
        self.media.iter().find(|m| m.media_type == "audio")
    }

    /// Get the rsaaeskey attribute (encrypted AES key)
    #[must_use]
    pub fn rsaaeskey(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("rsaaeskey")?
            .as_deref()
            .or_else(|| self.get_attribute("rsaaeskey"))
    }

    /// Get the aesiv attribute (AES initialization vector)
    #[must_use]
    pub fn aesiv(&self) -> Option<&str> {
        self.audio_media()?
            .attributes
            .get("aesiv")?
            .as_deref()
            .or_else(|| self.get_attribute("aesiv"))
    }

    /// Get the fmtp attribute (format parameters)
    #[must_use]
    pub fn fmtp(&self) -> Option<&str> {
        self.audio_media()?.attributes.get("fmtp")?.as_deref()
    }

    /// Get the rtpmap attribute
    #[must_use]
    pub fn rtpmap(&self) -> Option<&str> {
        self.audio_media()?.attributes.get("rtpmap")?.as_deref()
    }
}

impl MediaDescription {
    /// Get a media-level attribute
    #[must_use]
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name)?.as_deref()
    }
}
