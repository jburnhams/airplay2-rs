use std::collections::HashMap;

use thiserror::Error;

use super::{MediaDescription, SdpConnection, SdpOrigin, SessionDescription};

#[derive(Debug, Error)]
pub enum SdpParseError {
    #[error("invalid version line")]
    InvalidVersion,
    #[error("invalid origin line: {0}")]
    InvalidOrigin(String),
    #[error("invalid connection line: {0}")]
    InvalidConnection(String),
    #[error("invalid media line: {0}")]
    InvalidMedia(String),
    #[error("invalid attribute: {0}")]
    InvalidAttribute(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

/// SDP parser
pub struct SdpParser;

impl SdpParser {
    /// Parse SDP from string
    ///
    /// # Errors
    ///
    /// Returns `SdpParseError` if the input is not a valid SDP string.
    pub fn parse(input: &str) -> Result<SessionDescription, SdpParseError> {
        let mut sdp = SessionDescription::default();
        let mut current_media: Option<MediaDescription> = None;

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.len() < 2 || line.chars().nth(1) != Some('=') {
                continue;
            }

            let type_char = line.chars().next().unwrap_or(' ');
            let value = &line[2..];

            match type_char {
                'v' => {
                    sdp.version = value.parse().map_err(|_| SdpParseError::InvalidVersion)?;
                }
                'o' => {
                    sdp.origin = Some(Self::parse_origin(value)?);
                }
                's' => {
                    sdp.session_name = value.to_string();
                }
                'c' => {
                    let conn = Self::parse_connection(value)?;
                    if current_media.is_some() {
                        // Connection for current media
                    } else {
                        sdp.connection = Some(conn);
                    }
                }
                't' => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        sdp.timing =
                            Some((parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0)));
                    }
                }
                'm' => {
                    // Save previous media if any
                    if let Some(media) = current_media.take() {
                        sdp.media.push(media);
                    }
                    current_media = Some(Self::parse_media(value)?);
                }
                'a' => {
                    let (name, value) = Self::parse_attribute(value);
                    if let Some(ref mut media) = current_media {
                        media.attributes.insert(name, value);
                    } else {
                        sdp.attributes.insert(name, value);
                    }
                }
                _ => {
                    // Ignore unknown lines
                }
            }
        }

        // Save last media
        if let Some(media) = current_media {
            sdp.media.push(media);
        }

        Ok(sdp)
    }

    fn parse_origin(value: &str) -> Result<SdpOrigin, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 6 {
            return Err(SdpParseError::InvalidOrigin(value.to_string()));
        }

        Ok(SdpOrigin {
            username: parts[0].to_string(),
            session_id: parts[1].to_string(),
            session_version: parts[2].to_string(),
            net_type: parts[3].to_string(),
            addr_type: parts[4].to_string(),
            unicast_address: parts[5].to_string(),
        })
    }

    fn parse_connection(value: &str) -> Result<SdpConnection, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(SdpParseError::InvalidConnection(value.to_string()));
        }

        Ok(SdpConnection {
            net_type: parts[0].to_string(),
            addr_type: parts[1].to_string(),
            address: parts[2].to_string(),
        })
    }

    fn parse_media(value: &str) -> Result<MediaDescription, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(SdpParseError::InvalidMedia(value.to_string()));
        }

        Ok(MediaDescription {
            media_type: parts[0].to_string(),
            port: parts[1].parse().unwrap_or(0),
            protocol: parts[2].to_string(),
            formats: parts[3..].iter().map(ToString::to_string).collect(),
            attributes: HashMap::new(),
        })
    }

    fn parse_attribute(value: &str) -> (String, Option<String>) {
        if let Some(colon_pos) = value.find(':') {
            let name = value[..colon_pos].to_string();
            let attr_value = value[colon_pos + 1..].to_string();
            (name, Some(attr_value))
        } else {
            (value.to_string(), None)
        }
    }
}
