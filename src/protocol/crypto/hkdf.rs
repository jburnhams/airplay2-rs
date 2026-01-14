use super::CryptoError;
use hkdf::Hkdf;
use sha2::Sha512;

/// HKDF-SHA512 for key derivation
pub struct HkdfSha512 {
    hkdf: Hkdf<Sha512>,
}

impl HkdfSha512 {
    /// Create HKDF instance from input key material
    ///
    /// # Arguments
    /// * `salt` - Optional salt (can be None or empty)
    /// * `ikm` - Input key material
    pub fn new(salt: Option<&[u8]>, ikm: &[u8]) -> Self {
        let hkdf = Hkdf::<Sha512>::new(salt, ikm);
        Self { hkdf }
    }

    /// Expand to derive output key material
    ///
    /// # Arguments
    /// * `info` - Context/application-specific info
    /// * `length` - Desired output length
    pub fn expand(&self, info: &[u8], length: usize) -> Result<Vec<u8>, CryptoError> {
        let mut okm = vec![0u8; length];
        self.hkdf
            .expand(info, &mut okm)
            .map_err(|_| CryptoError::KeyDerivationFailed("HKDF expand failed".into()))?;
        Ok(okm)
    }

    /// Expand into fixed-size array
    pub fn expand_fixed<const N: usize>(&self, info: &[u8]) -> Result<[u8; N], CryptoError> {
        let mut okm = [0u8; N];
        self.hkdf
            .expand(info, &mut okm)
            .map_err(|_| CryptoError::KeyDerivationFailed("HKDF expand failed".into()))?;
        Ok(okm)
    }
}

/// Convenience function for one-shot key derivation
pub fn derive_key(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    length: usize,
) -> Result<Vec<u8>, CryptoError> {
    HkdfSha512::new(salt, ikm).expand(info, length)
}

/// Derive `AirPlay` session keys from shared secret
///
/// `AirPlay` uses specific info strings for different keys
pub struct AirPlayKeys {
    /// Key for encrypting messages to device
    pub output_key: [u8; 32],
    /// Key for decrypting messages from device
    pub input_key: [u8; 32],
}

impl AirPlayKeys {
    /// Derive keys from shared secret using AirPlay-specific info strings
    pub fn derive(shared_secret: &[u8], salt: &[u8]) -> Result<Self, CryptoError> {
        let hkdf = HkdfSha512::new(Some(salt), shared_secret);

        // AirPlay uses specific info strings
        let output_key = hkdf.expand_fixed::<32>(b"ServerEncrypt-main")?;
        let input_key = hkdf.expand_fixed::<32>(b"ClientEncrypt-main")?;

        Ok(Self {
            output_key,
            input_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hkdf_derive() {
        let ikm = b"input key material";
        let salt = b"salt";
        let info = b"info";

        let key = derive_key(Some(salt), ikm, info, 32).unwrap();

        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_hkdf_deterministic() {
        let ikm = b"test";

        let key1 = derive_key(None, ikm, b"info", 32).unwrap();
        let key2 = derive_key(None, ikm, b"info", 32).unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_hkdf_different_info() {
        let ikm = b"test";

        let key1 = derive_key(None, ikm, b"info1", 32).unwrap();
        let key2 = derive_key(None, ikm, b"info2", 32).unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_airplay_keys() {
        let shared_secret = [0x42u8; 32];
        let salt = [0x00u8; 32];

        let keys = AirPlayKeys::derive(&shared_secret, &salt).unwrap();

        assert_eq!(keys.output_key.len(), 32);
        assert_eq!(keys.input_key.len(), 32);
        assert_ne!(keys.output_key, keys.input_key);
    }
}
