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
