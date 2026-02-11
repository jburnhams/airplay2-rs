//! TLV8 encoding for `HomeKit` pairing protocol

use std::collections::HashMap;
use thiserror::Error;

/// TLV type codes used in `HomeKit` pairing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TlvType {
    /// Method to use (pairing method)
    Method = 0x00,
    /// Pairing identifier
    Identifier = 0x01,
    /// Salt for SRP
    Salt = 0x02,
    /// Public key
    PublicKey = 0x03,
    /// Proof (M1/M2 in SRP)
    Proof = 0x04,
    /// Encrypted data
    EncryptedData = 0x05,
    /// Pairing state/sequence number
    State = 0x06,
    /// Error code
    Error = 0x07,
    /// Retry delay
    RetryDelay = 0x08,
    /// Certificate
    Certificate = 0x09,
    /// Signature
    Signature = 0x0A,
    /// Permissions
    Permissions = 0x0B,
    /// Fragment data
    FragmentData = 0x0C,
    /// Fragment last
    FragmentLast = 0x0D,
    /// Session ID
    SessionID = 0x0E,
    /// Flags
    Flags = 0x13,
    /// Separator (empty value, used to separate items)
    Separator = 0xFF,
}

impl TlvType {
    /// Create from byte value
    #[must_use]
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Method),
            0x01 => Some(Self::Identifier),
            0x02 => Some(Self::Salt),
            0x03 => Some(Self::PublicKey),
            0x04 => Some(Self::Proof),
            0x05 => Some(Self::EncryptedData),
            0x06 => Some(Self::State),
            0x07 => Some(Self::Error),
            0x08 => Some(Self::RetryDelay),
            0x09 => Some(Self::Certificate),
            0x0A => Some(Self::Signature),
            0x0B => Some(Self::Permissions),
            0x0C => Some(Self::FragmentData),
            0x0D => Some(Self::FragmentLast),
            0x0E => Some(Self::SessionID),
            0x13 => Some(Self::Flags),
            0xFF => Some(Self::Separator),
            _ => None,
        }
    }
}

/// TLV encoding errors
#[derive(Debug, Error)]
pub enum TlvError {
    #[error("buffer too small")]
    BufferTooSmall,

    #[error("invalid TLV structure")]
    InvalidStructure,

    #[error("unknown type: 0x{0:02x}")]
    UnknownType(u8),

    #[error("missing required field: {0:?}")]
    MissingField(TlvType),

    #[error("invalid value for {0:?}")]
    InvalidValue(TlvType),
}

/// TLV encoder
pub struct TlvEncoder {
    buffer: Vec<u8>,
}

impl TlvEncoder {
    /// Create a new encoder
    #[must_use]
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Add a TLV item
    #[must_use]
    pub fn add(mut self, tlv_type: TlvType, value: &[u8]) -> Self {
        // TLV8 limits each chunk to 255 bytes
        // For larger values, we need to fragment across multiple TLVs
        for chunk in value.chunks(255) {
            self.buffer.push(tlv_type as u8);
            #[allow(clippy::cast_possible_truncation)]
            self.buffer.push(chunk.len() as u8);
            self.buffer.extend_from_slice(chunk);
        }

        // Handle empty value
        if value.is_empty() {
            self.buffer.push(tlv_type as u8);
            self.buffer.push(0);
        }

        self
    }

    /// Add a single byte value
    #[must_use]
    pub fn add_byte(self, tlv_type: TlvType, value: u8) -> Self {
        self.add(tlv_type, &[value])
    }

    /// Add a single byte value (alias for `add_byte`)
    #[must_use]
    pub fn add_u8(self, tlv_type: TlvType, value: u8) -> Self {
        self.add_byte(tlv_type, value)
    }

    /// Add bytes (alias for `add`)
    #[must_use]
    pub fn add_bytes(self, tlv_type: TlvType, value: &[u8]) -> Self {
        self.add(tlv_type, value)
    }

    /// Build the encoded TLV data (alias for `build`)
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        self.build()
    }

    /// Add state value
    #[must_use]
    pub fn add_state(self, state: u8) -> Self {
        self.add_byte(TlvType::State, state)
    }

    /// Add method value
    #[must_use]
    pub fn add_method(self, method: u8) -> Self {
        self.add_byte(TlvType::Method, method)
    }

    /// Build the encoded TLV data
    #[must_use]
    pub fn build(self) -> Vec<u8> {
        self.buffer
    }
}

impl Default for TlvEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// TLV decoder
pub struct TlvDecoder {
    items: HashMap<u8, Vec<u8>>,
}

impl TlvDecoder {
    /// Decode TLV data
    ///
    /// # Errors
    ///
    /// Returns error if buffer is too small or malformed
    pub fn decode(data: &[u8]) -> Result<Self, TlvError> {
        let mut items: HashMap<u8, Vec<u8>> = HashMap::new();
        let mut pos = 0;

        while pos < data.len() {
            if pos + 2 > data.len() {
                // If only 1 byte left (type), it's incomplete
                // If type present but length missing, incomplete
                // But wait, what if data is [type, 0] ? length is 2.
                // pos=0. data.len()=2. pos+2=2 <= 2. OK.
                // So this check is for at least 2 bytes remaining.
                return Err(TlvError::BufferTooSmall);
            }

            let tlv_type = data[pos];
            let length = data[pos + 1] as usize;
            pos += 2;

            if pos + length > data.len() {
                return Err(TlvError::BufferTooSmall);
            }

            let value = &data[pos..pos + length];
            pos += length;

            // Concatenate fragmented values
            items.entry(tlv_type).or_default().extend_from_slice(value);
        }

        Ok(Self { items })
    }

    /// Get a value by type
    #[must_use]
    pub fn get(&self, tlv_type: TlvType) -> Option<&[u8]> {
        self.items
            .get(&(tlv_type as u8))
            .map(std::vec::Vec::as_slice)
    }

    /// Get a value by type (alias for get)
    #[must_use]
    pub fn get_bytes(&self, tlv_type: TlvType) -> Option<&[u8]> {
        self.get(tlv_type)
    }

    /// Get a single byte value
    #[must_use]
    pub fn get_u8(&self, tlv_type: TlvType) -> Option<u8> {
        self.get(tlv_type).and_then(|v| v.first().copied())
    }

    /// Get a required value
    ///
    /// # Errors
    ///
    /// Returns error if field is missing
    pub fn get_required(&self, tlv_type: TlvType) -> Result<&[u8], TlvError> {
        self.get(tlv_type).ok_or(TlvError::MissingField(tlv_type))
    }

    /// Get state value
    ///
    /// # Errors
    ///
    /// Returns error if state field is missing or invalid length
    pub fn get_state(&self) -> Result<u8, TlvError> {
        let value = self.get_required(TlvType::State)?;
        if value.len() != 1 {
            return Err(TlvError::InvalidValue(TlvType::State));
        }
        Ok(value[0])
    }

    /// Get error value (if present)
    #[must_use]
    pub fn get_error(&self) -> Option<u8> {
        self.get(TlvType::Error).and_then(|v| v.first().copied())
    }

    /// Check if an error is present
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.get(TlvType::Error).is_some()
    }
}

/// Pairing method constants
pub mod methods {
    /// Pair-Setup
    pub const PAIR_SETUP: u8 = 0;
    /// Pair-Setup with auth (`MFi`)
    pub const PAIR_SETUP_AUTH: u8 = 1;
    /// Pair-Verify
    pub const PAIR_VERIFY: u8 = 2;
    /// Add pairing
    pub const ADD_PAIRING: u8 = 3;
    /// Remove pairing
    pub const REMOVE_PAIRING: u8 = 4;
    /// List pairings
    pub const LIST_PAIRINGS: u8 = 5;
}

/// Error codes from device
pub mod errors {
    pub const UNKNOWN: u8 = 0x01;
    pub const AUTHENTICATION: u8 = 0x02;
    pub const BACKOFF: u8 = 0x03;
    pub const MAX_PEERS: u8 = 0x04;
    pub const MAX_TRIES: u8 = 0x05;
    pub const UNAVAILABLE: u8 = 0x06;
    pub const BUSY: u8 = 0x07;
}
