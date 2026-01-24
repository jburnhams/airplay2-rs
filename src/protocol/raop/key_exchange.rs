//! AES key exchange for RAOP audio encryption

use super::super::crypto::AppleRsaPublicKey;
use super::super::crypto::CryptoError;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD as BASE64};

/// AES key size (128 bits)
pub const AES_KEY_SIZE: usize = 16;
/// AES IV size (128 bits)
pub const AES_IV_SIZE: usize = 16;

/// Session keys for RAOP audio encryption
#[derive(Clone)]
pub struct RaopSessionKeys {
    /// AES encryption key
    pub(crate) aes_key: [u8; AES_KEY_SIZE],
    /// AES initialization vector
    pub(crate) aes_iv: [u8; AES_IV_SIZE],
    /// RSA-encrypted AES key (for SDP)
    pub(crate) encrypted_key: Vec<u8>,
}

impl RaopSessionKeys {
    /// Generate new random session keys
    ///
    /// # Errors
    ///
    /// Returns `CryptoError` if key generation or encryption fails.
    pub fn generate() -> Result<Self, CryptoError> {
        use rand::RngCore;

        let mut aes_key = [0u8; AES_KEY_SIZE];
        let mut aes_iv = [0u8; AES_IV_SIZE];

        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut aes_key);
        rng.fill_bytes(&mut aes_iv);

        // Encrypt AES key with Apple's RSA public key
        let public_key = AppleRsaPublicKey::load()?;
        let encrypted_key = public_key.encrypt_oaep(&aes_key)?;

        Ok(Self {
            aes_key,
            aes_iv,
            encrypted_key,
        })
    }

    /// Get the AES key
    #[must_use]
    pub fn aes_key(&self) -> &[u8; AES_KEY_SIZE] {
        &self.aes_key
    }

    /// Get the AES IV
    #[must_use]
    pub fn aes_iv(&self) -> &[u8; AES_IV_SIZE] {
        &self.aes_iv
    }

    /// Get RSA-encrypted AES key as Base64 for `rsaaeskey` SDP attribute
    #[must_use]
    pub fn rsaaeskey(&self) -> String {
        BASE64.encode(&self.encrypted_key)
    }

    /// Get AES IV as Base64 for `aesiv` SDP attribute
    #[must_use]
    pub fn aesiv(&self) -> String {
        BASE64.encode(self.aes_iv)
    }
}

impl Drop for RaopSessionKeys {
    fn drop(&mut self) {
        // Zeroize sensitive data
        self.aes_key.iter_mut().for_each(|b| *b = 0);
        self.aes_iv.iter_mut().for_each(|b| *b = 0);
    }
}

/// Parse `rsaaeskey` and `aesiv` from SDP (server-side)
///
/// # Errors
///
/// Returns `CryptoError` if parsing or decryption fails.
pub fn parse_session_keys(
    rsaaeskey_b64: &str,
    aesiv_b64: &str,
    private_key: &super::super::crypto::RaopRsaPrivateKey,
) -> Result<([u8; AES_KEY_SIZE], [u8; AES_IV_SIZE]), CryptoError> {
    // Decode and decrypt AES key
    let encrypted_key = BASE64
        .decode(rsaaeskey_b64.trim())
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid base64: {e}")))?;

    let aes_key_vec = private_key.decrypt_oaep(&encrypted_key)?;

    if aes_key_vec.len() != AES_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength {
            expected: AES_KEY_SIZE,
            actual: aes_key_vec.len(),
        });
    }

    let mut aes_key = [0u8; AES_KEY_SIZE];
    aes_key.copy_from_slice(&aes_key_vec);

    // Decode AES IV
    let aes_iv_vec = BASE64
        .decode(aesiv_b64.trim())
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid base64: {e}")))?;

    if aes_iv_vec.len() != AES_IV_SIZE {
        return Err(CryptoError::InvalidKeyLength {
            expected: AES_IV_SIZE,
            actual: aes_iv_vec.len(),
        });
    }

    let mut aes_iv = [0u8; AES_IV_SIZE];
    aes_iv.copy_from_slice(&aes_iv_vec);

    Ok((aes_key, aes_iv))
}
