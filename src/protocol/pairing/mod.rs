//! `HomeKit` pairing protocol implementation

pub mod setup;
pub mod storage;
pub mod tlv;
pub mod transient;
pub mod verify;

#[cfg(test)]
mod tests;

pub use setup::PairSetup;
pub use storage::{PairingKeys, PairingStorage};
pub use tlv::{TlvDecoder, TlvEncoder, TlvError, TlvType};
pub use transient::TransientPairing;
pub use verify::PairVerify;

use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};

/// Pairing session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    /// Initial state
    Init,
    /// Waiting for device response
    WaitingResponse,
    /// SRP exchange in progress (Pair-Setup)
    SrpExchange,
    /// Key exchange in progress
    KeyExchange,
    /// Verifying signatures
    Verifying,
    /// Pairing complete
    Complete,
    /// Pairing failed
    Failed,
}

/// Result of a pairing step
#[derive(Debug)]
pub enum PairingStepResult {
    /// Need to send data to device
    SendData(Vec<u8>),
    /// Need more data from device
    NeedData,
    /// Pairing complete, here are the session keys
    Complete(SessionKeys),
    /// Pairing failed
    Failed(PairingError),
}

/// Established session keys after pairing
#[derive(Clone, Debug)]
pub struct SessionKeys {
    /// Key for encrypting data sent to device
    pub encrypt_key: [u8; 32],
    /// Key for decrypting data from device
    pub decrypt_key: [u8; 32],
    /// Initial nonce for encryption
    pub encrypt_nonce: u64,
    /// Initial nonce for decryption
    pub decrypt_nonce: u64,
}

impl SessionKeys {
    /// Create cipher for encrypting outgoing messages
    ///
    /// # Errors
    ///
    /// Returns error if key is invalid
    pub fn encryptor(&self) -> Result<EncryptedChannel, crate::protocol::crypto::CryptoError> {
        EncryptedChannel::new(&self.encrypt_key, self.encrypt_nonce, true)
    }

    /// Create cipher for decrypting incoming messages
    ///
    /// # Errors
    ///
    /// Returns error if key is invalid
    pub fn decryptor(&self) -> Result<EncryptedChannel, crate::protocol::crypto::CryptoError> {
        EncryptedChannel::new(&self.decrypt_key, self.decrypt_nonce, false)
    }
}

/// Encrypted channel for post-pairing communication
pub struct EncryptedChannel {
    cipher: ChaCha20Poly1305Cipher,
    nonce_counter: u64,
    #[allow(dead_code)] // Reserved for future use
    is_sender: bool,
}

impl EncryptedChannel {
    /// Create a new encrypted channel
    ///
    /// # Errors
    ///
    /// Returns error if key is invalid length
    pub fn new(
        key: &[u8],
        initial_nonce: u64,
        is_sender: bool,
    ) -> Result<Self, crate::protocol::crypto::CryptoError> {
        let cipher = ChaCha20Poly1305Cipher::new(key)?;
        Ok(Self {
            cipher,
            nonce_counter: initial_nonce,
            is_sender,
        })
    }

    /// Encrypt a message
    ///
    /// # Errors
    ///
    /// Returns error if encryption fails
    pub fn encrypt(
        &mut self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let nonce = Nonce::from_counter(self.nonce_counter);
        self.nonce_counter += 1;
        self.cipher.encrypt(&nonce, plaintext)
    }

    /// Decrypt a message
    ///
    /// # Errors
    ///
    /// Returns error if decryption fails or authentication tag is invalid
    pub fn decrypt(
        &mut self,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let nonce = Nonce::from_counter(self.nonce_counter);
        self.nonce_counter += 1;
        self.cipher.decrypt(&nonce, ciphertext)
    }

    /// Encrypt with length prefix (for framed protocols)
    ///
    /// # Errors
    ///
    /// Returns error if encryption fails
    pub fn encrypt_framed(
        &mut self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let encrypted = self.encrypt(plaintext)?;
        if encrypted.len() > u16::MAX as usize {
            return Err(crate::protocol::crypto::CryptoError::EncryptionFailed(
                "Message too long for framing".to_string(),
            ));
        }
        #[allow(clippy::cast_possible_truncation)]
        let len_u16 = encrypted.len() as u16;

        let mut output = Vec::with_capacity(2 + encrypted.len());
        output.extend_from_slice(&len_u16.to_le_bytes());
        output.extend_from_slice(&encrypted);
        Ok(output)
    }
}

/// Pairing errors
#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("invalid state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("invalid TLV: {0}")]
    InvalidTlv(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("SRP verification failed")]
    SrpVerificationFailed,

    #[error("signature verification failed")]
    SignatureVerificationFailed,

    #[error("crypto error: {0}")]
    CryptoError(#[from] crate::protocol::crypto::CryptoError),

    #[error("device returned error: {code}")]
    DeviceError { code: u8 },

    #[error("pairing not supported by device")]
    NotSupported,

    #[error("pairing required (no stored keys)")]
    PairingRequired,

    #[error("stored keys invalid")]
    InvalidStoredKeys,

    #[error("TLV error: {0}")]
    Tlv(#[from] tlv::TlvError),
}
