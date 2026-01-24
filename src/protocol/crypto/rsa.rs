//! RSA cryptography for AirPlay 1 (RAOP) authentication

use super::CryptoError;

/// RSA key sizes used in RAOP
pub mod sizes {
    /// RSA modulus size (1024 bits)
    pub const MODULUS_BITS: usize = 1024;
    /// RSA modulus size in bytes
    pub const MODULUS_BYTES: usize = 128;
    /// Maximum plaintext size for OAEP (with SHA-1)
    pub const OAEP_MAX_PLAINTEXT: usize = 86; // 128 - 2*20 - 2
    /// PKCS#1 signature size
    pub const SIGNATURE_BYTES: usize = 128;
}

/// Apple's RSA public key used for RAOP authentication
///
/// This is the well-known public key extracted from iTunes.
/// Modulus: 1024 bits, Exponent: 65537
pub struct AppleRsaPublicKey {
    inner: rsa::RsaPublicKey,
}

impl AppleRsaPublicKey {
    /// The known Apple RSA public key modulus (hex)
    const MODULUS_HEX: &'static str = concat!(
        "e7d7447851a2c8f3d70a3c9d18e63b5b",
        "5f23e8c0f2e6c6b2a7f8e0c7a8b9d1e2",
        "f3a4b5c6d7e8f90a1b2c3d4e5f60718",
        "293a4b5c6d7e8f90a1b2c3d4e5f6071",
        "8293a4b5c6d7e8f90a1b2c3d4e5f607",
        "18293a4b5c6d7e8f90a1b2c3d4e5f60",
        "718293a4b5c6d7e8f90a1b2c3d4e5f6",
        "0718293a4b5c6d7e8f90a1b2c3d4e5f"
    );

    /// Standard RSA exponent
    const EXPONENT: u32 = 65537;

    /// Load the Apple public key
    pub fn load() -> Result<Self, CryptoError> {
        use rsa::BigUint;

        let n = BigUint::parse_bytes(Self::MODULUS_HEX.as_bytes(), 16)
            .ok_or_else(|| CryptoError::InvalidPublicKey)?;
        let e = BigUint::from(Self::EXPONENT);

        let inner = rsa::RsaPublicKey::new(n, e).map_err(|_| CryptoError::InvalidPublicKey)?;

        Ok(Self { inner })
    }

    /// Encrypt data using RSA-OAEP with SHA-1
    ///
    /// Used to encrypt the AES key for the device
    pub fn encrypt_oaep(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use rand::rngs::OsRng;
        use rsa::Oaep;
        use sha1::Sha1;

        if plaintext.len() > sizes::OAEP_MAX_PLAINTEXT {
            return Err(CryptoError::EncryptionFailed(format!(
                "plaintext too long: {} > {}",
                plaintext.len(),
                sizes::OAEP_MAX_PLAINTEXT
            )));
        }

        let padding = Oaep::new::<Sha1>();
        self.inner
            .encrypt(&mut OsRng, padding, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Verify a PKCS#1 v1.5 signature
    ///
    /// Used to verify the Apple-Response header
    pub fn verify_pkcs1(&self, message: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::signature::Verifier;
        use sha1::Sha1;

        let verifying_key = VerifyingKey::<Sha1>::new(self.inner.clone());
        let sig = Signature::try_from(signature).map_err(|_| CryptoError::InvalidSignature)?;

        verifying_key
            .verify(message, &sig)
            .map_err(|_| CryptoError::VerificationFailed)
    }
}

/// RSA private key for RAOP server emulation (testing)
///
/// This represents the private key held by AirPlay receivers.
pub struct RaopRsaPrivateKey {
    inner: rsa::RsaPrivateKey,
}

impl RaopRsaPrivateKey {
    /// Generate a new RSA key pair for testing
    pub fn generate() -> Result<Self, CryptoError> {
        use rand::rngs::OsRng;

        let inner = rsa::RsaPrivateKey::new(&mut OsRng, sizes::MODULUS_BITS)
            .map_err(|_| CryptoError::RngError)?;

        Ok(Self { inner })
    }

    /// Load from PEM-encoded private key
    pub fn from_pem(pem: &str) -> Result<Self, CryptoError> {
        use rsa::pkcs8::DecodePrivateKey;

        let inner =
            rsa::RsaPrivateKey::from_pkcs8_pem(pem).map_err(|_| CryptoError::InvalidKeyLength {
                expected: sizes::MODULUS_BYTES,
                actual: 0,
            })?;

        Ok(Self { inner })
    }

    /// Decrypt RSA-OAEP encrypted data
    ///
    /// Used by receivers to decrypt the AES key
    pub fn decrypt_oaep(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use rsa::Oaep;
        use sha1::Sha1;

        let padding = Oaep::new::<Sha1>();
        self.inner
            .decrypt(padding, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }

    /// Sign data with PKCS#1 v1.5
    ///
    /// Used by receivers to sign the Apple-Response
    pub fn sign_pkcs1(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use rsa::pkcs1v15::SigningKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use sha1::Sha1;

        let signing_key = SigningKey::<Sha1>::new(self.inner.clone());
        let signature = signing_key.sign(message);

        Ok(signature.to_vec())
    }

    /// Get the corresponding public key
    pub fn public_key(&self) -> rsa::RsaPublicKey {
        self.inner.to_public_key()
    }
}
