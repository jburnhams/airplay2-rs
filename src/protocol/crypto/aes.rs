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
    fn test_aes_ctr_process() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let data = b"hello world";

        let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
        let ciphertext = cipher1.process(data);

        assert_ne!(ciphertext, data);

        let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();
        let plaintext = cipher2.process(&ciphertext);

        assert_eq!(plaintext, data);
    }

    #[test]
    fn test_aes_ctr_in_place() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let data = b"hello world";
        let mut buf = data.to_vec();

        let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
        cipher1.apply_keystream(&mut buf);

        assert_ne!(buf, data);

        let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();
        cipher2.apply_keystream(&mut buf);

        assert_eq!(buf, data);
    }

    #[test]
    fn test_aes_ctr_seek() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let data = b"hello world";

        let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
        let full_ciphertext = cipher1.process(data);

        // Decrypt only last 5 bytes
        let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();
        let offset = (data.len() - 5) as u64;
        cipher2.seek(offset);

        let mut partial = full_ciphertext[full_ciphertext.len() - 5..].to_vec();
        cipher2.apply_keystream(&mut partial);

        assert_eq!(partial, &data[data.len() - 5..]);
    }

    #[test]
    fn test_aes_gcm() {
        let key = [0u8; 16];
        let nonce = [0u8; 12];
        let data = b"hello world";

        let cipher = Aes128Gcm::new(&key).unwrap();
        let encrypted = cipher.encrypt(&nonce, data).unwrap();

        let decrypted = cipher.decrypt(&nonce, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_aes_gcm_tamper() {
        let key = [0u8; 16];
        let nonce = [0u8; 12];
        let data = b"hello world";

        let cipher = Aes128Gcm::new(&key).unwrap();
        let mut encrypted = cipher.encrypt(&nonce, data).unwrap();

        // Tamper
        encrypted[0] ^= 0xFF;

        assert!(cipher.decrypt(&nonce, &encrypted).is_err());
    }
}
