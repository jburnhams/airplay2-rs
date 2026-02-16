//! Encrypted RTSP handling
//!
//! Wraps the RTSP server codec with the encryption layer to handle
//! encrypted control channel traffic.

use super::encrypted_channel::{EncryptedChannel, EncryptionError};
use crate::protocol::rtsp::server_codec::ParseError;
use crate::protocol::rtsp::{RtspRequest, RtspServerCodec};
use bytes::BytesMut;

/// RTSP codec with optional encryption
pub struct EncryptedRtspCodec {
    /// Underlying RTSP codec
    rtsp_codec: RtspServerCodec,
    /// Encryption channel
    channel: EncryptedChannel,
    /// Decrypted data buffer
    decrypted_buffer: BytesMut,
}

impl EncryptedRtspCodec {
    /// Create a new codec without encryption (pre-pairing)
    #[must_use]
    pub fn new() -> Self {
        Self {
            rtsp_codec: RtspServerCodec::new(),
            channel: EncryptedChannel::disabled(),
            decrypted_buffer: BytesMut::new(),
        }
    }

    /// Enable encryption with session keys
    pub fn enable_encryption(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.channel.enable(encrypt_key, decrypt_key);
        tracing::debug!("Control channel encryption enabled");
    }

    /// Disable encryption
    pub fn disable_encryption(&mut self) {
        self.channel.disable();
    }

    /// Check if encryption is enabled
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.channel.is_enabled()
    }

    /// Feed raw bytes from the network
    pub fn feed(&mut self, data: &[u8]) {
        self.channel.feed(data);
    }

    /// Try to decode the next RTSP request
    ///
    /// # Errors
    /// Returns `CodecError` if decryption or parsing fails.
    pub fn decode(&mut self) -> Result<Option<RtspRequest>, CodecError> {
        // Decrypt any available frames
        let frames = self.channel.decrypt_all().map_err(CodecError::Encryption)?;

        // Add decrypted data to buffer
        for frame in frames {
            self.decrypted_buffer.extend_from_slice(&frame);
        }

        // Feed to RTSP codec
        if !self.decrypted_buffer.is_empty() {
            let data = self.decrypted_buffer.split().to_vec();
            self.rtsp_codec.feed(&data);
        }

        // Try to decode
        self.rtsp_codec.decode().map_err(CodecError::Parse)
    }

    /// Encode a response (with encryption if enabled)
    ///
    /// # Errors
    /// Returns `CodecError` if encryption fails.
    pub fn encode_response(&mut self, response: &[u8]) -> Result<Vec<u8>, CodecError> {
        self.channel
            .encrypt(response)
            .map_err(CodecError::Encryption)
    }

    /// Clear buffers
    pub fn clear(&mut self) {
        self.rtsp_codec.clear();
        self.channel.clear();
        self.decrypted_buffer.clear();
    }
}

impl Default for EncryptedRtspCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors occurring in the encrypted RTSP codec
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// Error during encryption or decryption
    #[error("Encryption error: {0}")]
    Encryption(#[from] EncryptionError),

    /// Error during RTSP parsing
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
}

/// TCP connection handler with encryption support
pub struct EncryptedConnection {
    /// Codec for this connection
    codec: EncryptedRtspCodec,
    /// Peer address
    peer_addr: std::net::SocketAddr,
    /// Connection state
    state: ConnectionState,
}

/// State of the encrypted connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Pre-pairing - plaintext
    Plaintext,
    /// Post-pairing - encrypted
    Encrypted,
    /// Error state
    Error,
}

impl EncryptedConnection {
    /// Create a new connection for the given peer address
    #[must_use]
    pub fn new(peer_addr: std::net::SocketAddr) -> Self {
        Self {
            codec: EncryptedRtspCodec::new(),
            peer_addr,
            state: ConnectionState::Plaintext,
        }
    }

    /// Transition to encrypted state
    pub fn enable_encryption(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.codec.enable_encryption(encrypt_key, decrypt_key);
        self.state = ConnectionState::Encrypted;
        tracing::info!("Connection to {} now encrypted", self.peer_addr);
    }

    /// Process incoming data
    ///
    /// # Errors
    /// Returns `CodecError` if decryption or parsing fails.
    pub fn on_data(&mut self, data: &[u8]) -> Result<Vec<RtspRequest>, CodecError> {
        self.codec.feed(data);

        let mut requests = Vec::new();
        while let Some(request) = self.codec.decode()? {
            requests.push(request);
        }

        Ok(requests)
    }

    /// Encode response
    ///
    /// # Errors
    /// Returns `CodecError` if encryption fails.
    pub fn encode(&mut self, response: &[u8]) -> Result<Vec<u8>, CodecError> {
        self.codec.encode_response(response)
    }

    /// Get the peer address
    #[must_use]
    pub fn peer_addr(&self) -> std::net::SocketAddr {
        self.peer_addr
    }

    /// Get the current connection state
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state
    }
}
