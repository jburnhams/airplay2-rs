//! Encrypted Control Channel for `AirPlay` 2
//!
//! After pairing completes, all RTSP traffic is encrypted using
//! ChaCha20-Poly1305 with HAP-style framing.

use bytes::{Buf, BufMut, BytesMut};

use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};

/// Maximum frame size (64KB)
const MAX_FRAME_SIZE: usize = 65535;

/// Auth tag size for ChaCha20-Poly1305
const TAG_SIZE: usize = 16;

/// Length prefix size
const LENGTH_SIZE: usize = 2;

/// Encrypted channel state
pub struct EncryptedChannel {
    /// Key for encrypting outgoing messages
    encrypt_key: [u8; 32],
    /// Key for decrypting incoming messages
    decrypt_key: [u8; 32],
    /// Nonce counter for encryption
    encrypt_nonce: u64,
    /// Nonce counter for decryption
    decrypt_nonce: u64,
    /// Input buffer for frame reassembly
    input_buffer: BytesMut,
    /// Whether encryption is enabled
    enabled: bool,
}

impl EncryptedChannel {
    /// Create a new encrypted channel with derived keys
    ///
    /// # Arguments
    /// * `encrypt_key` - Key for encrypting messages TO the sender
    /// * `decrypt_key` - Key for decrypting messages FROM the sender
    #[must_use]
    pub fn new(encrypt_key: [u8; 32], decrypt_key: [u8; 32]) -> Self {
        Self {
            encrypt_key,
            decrypt_key,
            encrypt_nonce: 0,
            decrypt_nonce: 0,
            input_buffer: BytesMut::with_capacity(4096),
            enabled: true,
        }
    }

    /// Create a disabled/passthrough channel
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            encrypt_key: [0; 32],
            decrypt_key: [0; 32],
            encrypt_nonce: 0,
            decrypt_nonce: 0,
            input_buffer: BytesMut::new(),
            enabled: false,
        }
    }

    /// Check if encryption is enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable encryption with new keys
    pub fn enable(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.encrypt_key = encrypt_key;
        self.decrypt_key = decrypt_key;
        self.encrypt_nonce = 0;
        self.decrypt_nonce = 0;
        self.input_buffer.clear();
        self.enabled = true;
    }

    /// Disable encryption (passthrough mode)
    pub fn disable(&mut self) {
        self.enabled = false;
        self.input_buffer.clear();
    }

    /// Encrypt a message
    ///
    /// # Errors
    /// Returns `EncryptionError` if message is too large or encryption fails.
    ///
    /// # Panics
    /// Panics if the message length cannot be converted to `u16` after checking bounds.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if !self.enabled {
            return Ok(plaintext.to_vec());
        }

        if plaintext.len() > MAX_FRAME_SIZE {
            return Err(EncryptionError::MessageTooLarge {
                size: plaintext.len(),
                max: MAX_FRAME_SIZE,
            });
        }

        // Build nonce: 4 bytes zero + 8 bytes counter (LE)
        let nonce = Nonce::from_counter(self.encrypt_nonce);
        self.encrypt_nonce += 1;

        // Encrypt with AEAD
        let cipher = ChaCha20Poly1305Cipher::new(&self.encrypt_key)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        // Build frame: length (2 bytes LE) + ciphertext (includes tag)
        let mut frame = Vec::with_capacity(LENGTH_SIZE + ciphertext.len());
        // We've already checked that plaintext.len() <= MAX_FRAME_SIZE (u16::MAX),
        // so this cast is safe.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Checked against MAX_FRAME_SIZE"
        )]
        frame.put_u16_le(plaintext.len() as u16);

        frame.extend_from_slice(&ciphertext);

        Ok(frame)
    }

    /// Feed bytes into the decryption buffer
    pub fn feed(&mut self, data: &[u8]) {
        self.input_buffer.extend_from_slice(data);
    }

    /// Try to decrypt a complete frame from the buffer
    ///
    /// # Errors
    /// Returns `EncryptionError` if decryption fails or frame length is invalid.
    pub fn decrypt(&mut self) -> Result<Option<Vec<u8>>, EncryptionError> {
        if !self.enabled {
            // Passthrough mode - return entire buffer
            if self.input_buffer.is_empty() {
                return Ok(None);
            }
            let data = self.input_buffer.split().to_vec();
            return Ok(Some(data));
        }

        // Need at least length prefix
        if self.input_buffer.len() < LENGTH_SIZE {
            return Ok(None);
        }

        // Read length (peek, don't consume yet)
        let plaintext_len =
            u16::from_le_bytes([self.input_buffer[0], self.input_buffer[1]]) as usize;

        // Validate length
        if plaintext_len > MAX_FRAME_SIZE {
            return Err(EncryptionError::InvalidFrameLength(plaintext_len));
        }

        // Total frame size: length prefix + plaintext + auth tag
        let frame_size = LENGTH_SIZE + plaintext_len + TAG_SIZE;

        // Need complete frame
        if self.input_buffer.len() < frame_size {
            return Ok(None);
        }

        // Consume the frame
        let _ = self.input_buffer.get_u16_le(); // length prefix
        let ciphertext: Vec<u8> = self
            .input_buffer
            .split_to(plaintext_len + TAG_SIZE)
            .to_vec();

        // Build nonce
        let nonce = Nonce::from_counter(self.decrypt_nonce);
        self.decrypt_nonce += 1;

        // Decrypt with AEAD
        let cipher = ChaCha20Poly1305Cipher::new(&self.decrypt_key)
            .map_err(|_| EncryptionError::DecryptionFailed)?;
        let plaintext = cipher
            .decrypt(&nonce, &ciphertext)
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        Ok(Some(plaintext))
    }

    /// Decrypt all available frames
    ///
    /// # Errors
    /// Returns `EncryptionError` if any frame fails to decrypt.
    pub fn decrypt_all(&mut self) -> Result<Vec<Vec<u8>>, EncryptionError> {
        let mut frames = Vec::new();

        while let Some(frame) = self.decrypt()? {
            frames.push(frame);
        }

        Ok(frames)
    }

    /// Get current encrypt nonce (for debugging)
    #[must_use]
    pub fn encrypt_nonce(&self) -> u64 {
        self.encrypt_nonce
    }

    /// Get current decrypt nonce (for debugging)
    #[must_use]
    pub fn decrypt_nonce(&self) -> u64 {
        self.decrypt_nonce
    }

    /// Clear input buffer
    pub fn clear(&mut self) {
        self.input_buffer.clear();
    }
}

/// Errors related to encryption/decryption
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    /// Message too large
    #[error("Message too large: {size} bytes (max {max})")]
    MessageTooLarge {
        /// Actual size
        size: usize,
        /// Maximum allowed size
        max: usize,
    },

    /// Invalid frame length
    #[error("Invalid frame length: {0}")]
    InvalidFrameLength(usize),

    /// Encryption failed
    #[error("Encryption failed")]
    EncryptionFailed,

    /// Decryption failed
    #[error("Decryption failed - authentication error or corrupted data")]
    DecryptionFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_channel() -> (EncryptedChannel, EncryptedChannel) {
        // Create two channels with swapped keys (simulating sender/receiver)
        let key_a = [0x41u8; 32];
        let key_b = [0x42u8; 32];

        let sender = EncryptedChannel::new(key_a, key_b);
        let receiver = EncryptedChannel::new(key_b, key_a);

        (sender, receiver)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (mut sender, mut receiver) = create_test_channel();

        let message = b"Hello, AirPlay 2!";

        // Encrypt on sender side
        let encrypted = sender.encrypt(message).unwrap();

        // Decrypt on receiver side
        receiver.feed(&encrypted);
        let decrypted = receiver.decrypt().unwrap().unwrap();

        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_multiple_messages() {
        let (mut sender, mut receiver) = create_test_channel();

        let messages = vec![
            b"First message".to_vec(),
            b"Second message".to_vec(),
            b"Third message".to_vec(),
        ];

        // Encrypt all
        let mut encrypted = Vec::new();
        for msg in &messages {
            encrypted.extend_from_slice(&sender.encrypt(msg).unwrap());
        }

        // Feed all at once
        receiver.feed(&encrypted);

        // Decrypt all
        let decrypted = receiver.decrypt_all().unwrap();

        assert_eq!(decrypted.len(), 3);
        for (i, msg) in decrypted.iter().enumerate() {
            assert_eq!(msg, &messages[i]);
        }
    }

    #[test]
    fn test_partial_frame() {
        let (mut sender, mut receiver) = create_test_channel();

        let message = b"Test partial frame";
        let encrypted = sender.encrypt(message).unwrap();

        // Feed only part of the frame
        receiver.feed(&encrypted[..5]);
        assert!(receiver.decrypt().unwrap().is_none());

        // Feed the rest
        receiver.feed(&encrypted[5..]);
        let decrypted = receiver.decrypt().unwrap().unwrap();

        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_nonce_increment() {
        let (mut sender, _) = create_test_channel();

        assert_eq!(sender.encrypt_nonce(), 0);

        sender.encrypt(b"message 1").unwrap();
        assert_eq!(sender.encrypt_nonce(), 1);

        sender.encrypt(b"message 2").unwrap();
        assert_eq!(sender.encrypt_nonce(), 2);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut channel = EncryptedChannel::disabled();

        assert!(!channel.is_enabled());

        // Should pass through unchanged
        let message = b"Plaintext message";
        let encrypted = channel.encrypt(message).unwrap();
        assert_eq!(encrypted, message);

        channel.feed(message);
        let decrypted = channel.decrypt().unwrap().unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key_a = [0x41u8; 32];
        let key_b = [0x42u8; 32];
        let key_c = [0x43u8; 32];

        let mut sender = EncryptedChannel::new(key_a, key_b);
        let mut receiver = EncryptedChannel::new(key_a, key_c); // Wrong decrypt key

        let encrypted = sender.encrypt(b"Secret").unwrap();
        receiver.feed(&encrypted);

        // Decryption should fail authentication
        let result = receiver.decrypt();
        assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
    }

    #[test]
    fn test_nonce_format() {
        use crate::protocol::crypto::Nonce;
        let nonce = Nonce::from_counter(0x0102_0304_0506_0708);
        let nonce_bytes = nonce.as_bytes();

        // First 4 bytes zero, last 8 bytes are counter LE
        assert_eq!(nonce_bytes[0..4], [0, 0, 0, 0]);
        assert_eq!(
            nonce_bytes[4..12],
            [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
    }
}
