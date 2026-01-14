use super::{CryptoError, lengths};
use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};

type Aes128CtrImpl = ctr::Ctr64BE<Aes128>;

/// AES-128-CTR stream cipher for audio encryption
pub struct Aes128Ctr {
    cipher: Aes128CtrImpl,
}

impl Aes128Ctr {
    /// Create cipher with 16-byte key and 16-byte IV
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != lengths::AES_128_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_128_KEY,
                actual: key.len(),
            });
        }
        if iv.len() != 16 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 16,
                actual: iv.len(),
            });
        }

        let cipher =
            Aes128CtrImpl::new_from_slices(key, iv).map_err(|_| CryptoError::InvalidKeyLength {
                expected: 16,
                actual: key.len(),
            })?;

        Ok(Self { cipher })
    }

    /// Encrypt/decrypt in place (XOR with keystream)
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        self.cipher.apply_keystream(data);
    }

    /// Encrypt/decrypt, returning new buffer
    pub fn process(&mut self, data: &[u8]) -> Vec<u8> {
        let mut output = data.to_vec();
        self.apply_keystream(&mut output);
        output
    }

    /// Seek to position in keystream
    pub fn seek(&mut self, position: u64) {
        self.cipher.seek(position);
    }
}

/// AES-128-GCM AEAD cipher
pub struct Aes128Gcm {
    cipher: aes_gcm::Aes128Gcm,
}

impl Aes128Gcm {
    /// Create cipher with 16-byte key
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        use aes_gcm::KeyInit;

        if key.len() != lengths::AES_128_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_128_KEY,
                actual: key.len(),
            });
        }

        let cipher =
            aes_gcm::Aes128Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength {
                expected: 16,
                actual: key.len(),
            })?;

        Ok(Self { cipher })
    }

    /// Encrypt with 12-byte nonce
    pub fn encrypt(&self, nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::Aead;

        if nonce.len() != lengths::AES_GCM_NONCE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            });
        }

        self.cipher
            .encrypt(aes_gcm::Nonce::from_slice(nonce), plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Decrypt with 12-byte nonce
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::Aead;

        if nonce.len() != lengths::AES_GCM_NONCE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            });
        }

        self.cipher
            .decrypt(aes_gcm::Nonce::from_slice(nonce), ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_ctr_encrypt_decrypt() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
        let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();

        let plaintext = b"Hello, AirPlay audio!";
        let ciphertext = cipher1.process(plaintext);

        assert_ne!(&ciphertext, plaintext);

        let decrypted = cipher2.process(&ciphertext);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_ctr_in_place() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();

        let mut data = b"test data".to_vec();
        let original = data.clone();

        cipher.apply_keystream(&mut data);
        assert_ne!(data, original);

        // Reset cipher and decrypt
        let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();
        cipher.apply_keystream(&mut data);
        assert_eq!(data, original);
    }

    #[test]
    fn test_aes_gcm_encrypt_decrypt() {
        let key = [0x42u8; 16];
        let nonce = [0x00u8; 12];

        let cipher = Aes128Gcm::new(&key).unwrap();

        let plaintext = b"Secret audio data";
        let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();
        let decrypted = cipher.decrypt(&nonce, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_tamper_detection() {
        let key = [0x42u8; 16];
        let nonce = [0x00u8; 12];

        let cipher = Aes128Gcm::new(&key).unwrap();

        let mut ciphertext = cipher.encrypt(&nonce, b"data").unwrap();
        ciphertext[0] ^= 0xFF; // Tamper with ciphertext

        let result = cipher.decrypt(&nonce, &ciphertext);
        assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
    }
}
