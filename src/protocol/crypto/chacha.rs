use super::{CryptoError, lengths};
use chacha20poly1305::{
    ChaCha20Poly1305 as ChaChaImpl, Nonce as ChaChaNonce,
    aead::{Aead, KeyInit},
};

/// 12-byte nonce for ChaCha20-Poly1305
#[derive(Clone, Copy)]
pub struct Nonce([u8; 12]);

impl Nonce {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != lengths::CHACHA_NONCE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::CHACHA_NONCE,
                actual: bytes.len(),
            });
        }
        let mut arr = [0u8; 12];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Create from u64 counter (little-endian, padded)
    pub fn from_counter(counter: u64) -> Self {
        let mut arr = [0u8; 12];
        arr[4..12].copy_from_slice(&counter.to_le_bytes());
        Self(arr)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

/// ChaCha20-Poly1305 AEAD cipher
pub struct ChaCha20Poly1305Cipher {
    cipher: ChaChaImpl,
}

impl ChaCha20Poly1305Cipher {
    /// Create cipher with 32-byte key
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != lengths::CHACHA_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::CHACHA_KEY,
                actual: key.len(),
            });
        }

        let cipher =
            ChaChaImpl::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength {
                expected: 32,
                actual: key.len(),
            })?;

        Ok(Self { cipher })
    }

    /// Encrypt with authentication
    ///
    /// Returns ciphertext with appended 16-byte tag
    pub fn encrypt(&self, nonce: &Nonce, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.cipher
            .encrypt(ChaChaNonce::from_slice(&nonce.0), plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Encrypt with associated data
    pub fn encrypt_with_aad(
        &self,
        nonce: &Nonce,
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        use chacha20poly1305::aead::Payload;

        self.cipher
            .encrypt(
                ChaChaNonce::from_slice(&nonce.0),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Decrypt and verify authentication
    ///
    /// Input should be ciphertext with appended 16-byte tag
    pub fn decrypt(&self, nonce: &Nonce, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.cipher
            .decrypt(ChaChaNonce::from_slice(&nonce.0), ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }

    /// Decrypt with associated data
    pub fn decrypt_with_aad(
        &self,
        nonce: &Nonce,
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        use chacha20poly1305::aead::Payload;

        self.cipher
            .decrypt(
                ChaChaNonce::from_slice(&nonce.0),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chacha_encrypt_decrypt() {
        let key = [0u8; 32];
        let nonce = Nonce([1u8; 12]);
        let data = b"hello world";

        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
        let encrypted = cipher.encrypt(&nonce, data).unwrap();

        // Tag is 16 bytes
        assert_eq!(encrypted.len(), data.len() + 16);

        let decrypted = cipher.decrypt(&nonce, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_chacha_aad() {
        let key = [0u8; 32];
        let nonce = Nonce([1u8; 12]);
        let data = b"hello world";
        let aad = b"header data";

        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
        let encrypted = cipher.encrypt_with_aad(&nonce, aad, data).unwrap();

        let decrypted = cipher.decrypt_with_aad(&nonce, aad, &encrypted).unwrap();
        assert_eq!(decrypted, data);

        // Wrong AAD should fail
        assert!(
            cipher
                .decrypt_with_aad(&nonce, b"wrong", &encrypted)
                .is_err()
        );
    }

    #[test]
    fn test_chacha_tamper() {
        let key = [0u8; 32];
        let nonce = Nonce([1u8; 12]);
        let data = b"hello world";

        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
        let mut encrypted = cipher.encrypt(&nonce, data).unwrap();

        // Tamper with data
        encrypted[0] ^= 0xFF;

        assert!(cipher.decrypt(&nonce, &encrypted).is_err());
    }
}
