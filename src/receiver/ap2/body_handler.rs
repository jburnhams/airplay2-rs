//! Request/Response body handling for `AirPlay` 2
//!
//! `AirPlay` 2 uses binary plist (bplist00) format for most request and
//! response bodies. This module provides parsing and generation utilities.

use std::collections::HashMap;

use crate::protocol::plist::{self, PlistValue};

/// Content types used in `AirPlay` 2
pub mod content_types {
    /// Binary plist content type
    pub const BINARY_PLIST: &str = "application/x-apple-binary-plist";
    /// Octet stream content type
    pub const OCTET_STREAM: &str = "application/octet-stream";
    /// Text parameters content type
    pub const TEXT_PARAMETERS: &str = "text/parameters";
    /// SDP content type
    pub const SDP: &str = "application/sdp";
}

/// Parse a binary plist request body
///
/// # Errors
///
/// Returns `BodyParseError` if the body is invalid or cannot be parsed.
pub fn parse_bplist_body(body: &[u8]) -> Result<PlistValue, BodyParseError> {
    if body.is_empty() {
        return Ok(PlistValue::Dictionary(HashMap::new()));
    }

    // Check magic header
    if body.len() < 8 || &body[..6] != b"bplist" {
        return Err(BodyParseError::InvalidMagic);
    }

    plist::decode(body).map_err(|e| BodyParseError::DecodeError(e.to_string()))
}

/// Encode a plist value to binary plist bytes
///
/// # Errors
///
/// Returns `BodyParseError` if the value cannot be encoded.
pub fn encode_bplist_body(value: &PlistValue) -> Result<Vec<u8>, BodyParseError> {
    plist::encode(value).map_err(|e| BodyParseError::EncodeError(e.to_string()))
}

/// Parse text/parameters body (key: value format)
///
/// # Errors
///
/// Returns `BodyParseError` if the body contains invalid UTF-8.
pub fn parse_text_parameters(body: &[u8]) -> Result<HashMap<String, String>, BodyParseError> {
    let text = std::str::from_utf8(body).map_err(|_| BodyParseError::InvalidUtf8)?;

    let mut params = HashMap::new();

    for line in text.lines() {
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            params.insert(key, value);
        }
    }

    Ok(params)
}

/// Generate text/parameters body
#[must_use]
pub fn encode_text_parameters<S: std::hash::BuildHasher>(
    params: &HashMap<String, String, S>,
) -> Vec<u8> {
    use std::fmt::Write;
    let mut output = String::new();
    for (key, value) in params {
        let _ = write!(output, "{key}: {value}\r\n");
    }
    output.into_bytes()
}

/// Helper to extract typed values from plist dictionaries
pub trait PlistExt {
    /// Get a string value from the dictionary
    fn get_string(&self, key: &str) -> Option<&str>;
    /// Get an integer value from the dictionary
    fn get_int(&self, key: &str) -> Option<i64>;
    /// Get a byte slice from the dictionary
    fn get_bytes(&self, key: &str) -> Option<&[u8]>;
    /// Get a boolean value from the dictionary
    fn get_bool(&self, key: &str) -> Option<bool>;
    /// Get a dictionary from the dictionary
    fn get_dict(&self, key: &str) -> Option<&HashMap<String, PlistValue>>;
    /// Get an array from the dictionary
    fn get_array(&self, key: &str) -> Option<&Vec<PlistValue>>;
}

impl PlistExt for PlistValue {
    fn get_string(&self, key: &str) -> Option<&str> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::String(s)) = dict.get(key) {
                return Some(s.as_str());
            }
        }
        None
    }

    fn get_int(&self, key: &str) -> Option<i64> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::Integer(i)) = dict.get(key) {
                return Some(*i);
            }
        }
        None
    }

    fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::Data(data)) = dict.get(key) {
                return Some(data.as_slice());
            }
        }
        None
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::Boolean(b)) = dict.get(key) {
                return Some(*b);
            }
        }
        None
    }

    fn get_dict(&self, key: &str) -> Option<&HashMap<String, PlistValue>> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::Dictionary(d)) = dict.get(key) {
                return Some(d);
            }
        }
        None
    }

    fn get_array(&self, key: &str) -> Option<&Vec<PlistValue>> {
        if let PlistValue::Dictionary(dict) = self {
            if let Some(PlistValue::Array(a)) = dict.get(key) {
                return Some(a);
            }
        }
        None
    }
}

/// Builder for plist response bodies
#[derive(Debug, Default)]
pub struct PlistResponseBuilder {
    values: HashMap<String, PlistValue>,
}

impl PlistResponseBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string value
    #[must_use]
    pub fn string(mut self, key: &str, value: impl Into<String>) -> Self {
        self.values
            .insert(key.to_string(), PlistValue::String(value.into()));
        self
    }

    /// Add an integer value
    #[must_use]
    pub fn int(mut self, key: &str, value: i64) -> Self {
        self.values
            .insert(key.to_string(), PlistValue::Integer(value));
        self
    }

    /// Add a boolean value
    #[must_use]
    pub fn bool(mut self, key: &str, value: bool) -> Self {
        self.values
            .insert(key.to_string(), PlistValue::Boolean(value));
        self
    }

    /// Add a data value
    #[must_use]
    pub fn data(mut self, key: &str, value: Vec<u8>) -> Self {
        self.values.insert(key.to_string(), PlistValue::Data(value));
        self
    }

    /// Add a dictionary value
    #[must_use]
    pub fn dict(mut self, key: &str, value: HashMap<String, PlistValue>) -> Self {
        self.values
            .insert(key.to_string(), PlistValue::Dictionary(value));
        self
    }

    /// Build the `PlistValue`
    #[must_use]
    pub fn build(self) -> PlistValue {
        PlistValue::Dictionary(self.values)
    }

    /// Build and encode the plist
    ///
    /// # Errors
    ///
    /// Returns `BodyParseError` if the encoding fails.
    pub fn encode(self) -> Result<Vec<u8>, BodyParseError> {
        encode_bplist_body(&self.build())
    }
}

/// Errors occurring during body parsing or encoding
#[derive(Debug, thiserror::Error)]
pub enum BodyParseError {
    /// Invalid magic header
    #[error("Invalid binary plist magic header")]
    InvalidMagic,

    /// Decoding error
    #[error("Failed to decode plist: {0}")]
    DecodeError(String),

    /// Encoding error
    #[error("Failed to encode plist: {0}")]
    EncodeError(String),

    /// Invalid UTF-8
    #[error("Invalid UTF-8 in text body")]
    InvalidUtf8,

    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid type
    #[error("Invalid field type for {0}: expected {1}")]
    InvalidType(String, String),
}
