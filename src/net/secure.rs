//! HAP (`HomeKit` Accessory Protocol) secure session implementation
//!
//! Provides ChaCha20-Poly1305 encryption for RTSP control sessions
//! after successful SRP pairing.

use crate::error::AirPlayError;
use byteorder::{ByteOrder, LittleEndian};
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, Key, KeyInit, Nonce, Tag};

/// HAP secure session state
pub struct HapSecureSession {
    encrypt_cipher: ChaCha20Poly1305,
    decrypt_cipher: ChaCha20Poly1305,
    encrypt_count: u64,
    decrypt_count: u64,
}

impl HapSecureSession {
    /// Create a new secure session from shared keys
    #[must_use]
    pub fn new(encrypt_key: &[u8; 32], decrypt_key: &[u8; 32]) -> Self {
        Self {
            encrypt_cipher: ChaCha20Poly1305::new(Key::from_slice(encrypt_key)),
            decrypt_cipher: ChaCha20Poly1305::new(Key::from_slice(decrypt_key)),
            encrypt_count: 0,
            decrypt_count: 0,
        }
    }

    /// Encrypt data into HAP blocks
    ///
    /// Each block is maximum 1024 bytes and is prefixed with a 2-byte length.
    ///
    /// # Errors
    /// Returns an error if encryption fails.
    pub fn encrypt(&mut self, data: &[u8]) -> Result<Vec<u8>, AirPlayError> {
        let mut output = Vec::with_capacity(data.len() + (data.len() / 1024 + 1) * 18);

        for chunk in data.chunks(1024) {
            let len = u16::try_from(chunk.len()).map_err(|_| AirPlayError::RtspError {
                message: "Chunk size exceeds u16".to_string(),
                status_code: None,
            })?;
            let mut len_bytes = [0u8; 2];
            LittleEndian::write_u16(&mut len_bytes, len);

            let mut nonce_bytes = [0u8; 12];
            LittleEndian::write_u64(&mut nonce_bytes[4..12], self.encrypt_count);
            let nonce = Nonce::from_slice(&nonce_bytes);

            let mut buffer = chunk.to_vec();
            let tag = self
                .encrypt_cipher
                .encrypt_in_place_detached(nonce, &len_bytes, &mut buffer)
                .map_err(|_| AirPlayError::AuthenticationFailed {
                    message: "Encryption failed".to_string(),
                    recoverable: false,
                })?;

            output.extend_from_slice(&len_bytes);
            output.extend_from_slice(&buffer);
            output.extend_from_slice(tag.as_slice());

            self.encrypt_count += 1;
        }

        Ok(output)
    }

    /// Decrypt a single HAP block
    ///
    /// Returns (`decrypted_data`, `remaining_input`)
    ///
    /// # Errors
    /// Returns an error if decryption fails or buffer is too small.
    pub fn decrypt_block<'a>(
        &mut self,
        data: &'a [u8],
    ) -> Result<(Vec<u8>, &'a [u8]), AirPlayError> {
        if data.len() < 18 {
            return Err(AirPlayError::RtspError {
                message: "Buffer too small for HAP block".to_string(),
                status_code: None,
            });
        }

        let len = LittleEndian::read_u16(&data[0..2]) as usize;
        if data.len() < 2 + len + 16 {
            return Err(AirPlayError::RtspError {
                message: "Incomplete HAP block".to_string(),
                status_code: None,
            });
        }

        let mut nonce_bytes = [0u8; 12];
        LittleEndian::write_u64(&mut nonce_bytes[4..12], self.decrypt_count);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let mut buffer = data[2..2 + len].to_vec();
        let tag = Tag::from_slice(&data[2 + len..2 + len + 16]);

        self.decrypt_cipher
            .decrypt_in_place_detached(nonce, &data[0..2], &mut buffer, tag)
            .map_err(|_| AirPlayError::AuthenticationFailed {
                message: "Decryption failed".to_string(),
                recoverable: false,
            })?;

        self.decrypt_count += 1;

        Ok((buffer, &data[2 + len + 16..]))
    }
}
