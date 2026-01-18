use super::{CryptoError, lengths};
use ed25519_dalek::{Signer, Verifier};

/// Ed25519 key pair for signing
pub struct Ed25519KeyPair {
    signing_key: ed25519_dalek::SigningKey,
}

impl Ed25519KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Self {
        use rand::rngs::OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Create key pair from secret key bytes (32 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 32,
                actual: bytes.len(),
            });
        }

        let bytes: [u8; 32] = bytes.try_into().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&bytes);
        Ok(Self { signing_key })
    }

    /// Get the public key
    pub fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey {
            verifying_key: self.signing_key.verifying_key(),
        }
    }

    /// Get secret key bytes (for storage)
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Ed25519Signature {
        let sig = self.signing_key.sign(message);
        Ed25519Signature { inner: sig }
    }
}

/// Ed25519 public key for verification
#[derive(Clone)]
pub struct Ed25519PublicKey {
    verifying_key: ed25519_dalek::VerifyingKey,
}

impl Ed25519PublicKey {
    /// Create from bytes (32 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != lengths::ED25519_PUBLIC_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::ED25519_PUBLIC_KEY,
                actual: bytes.len(),
            });
        }

        let bytes: [u8; 32] = bytes.try_into().unwrap();
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map_err(|_| CryptoError::InvalidPublicKey)?;

        Ok(Self { verifying_key })
    }

    /// Get public key bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.verifying_key.as_bytes()
    }

    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature: &Ed25519Signature) -> Result<(), CryptoError> {
        self.verifying_key
            .verify(message, &signature.inner)
            .map_err(|_| CryptoError::InvalidSignature)
    }
}

/// Ed25519 signature
pub struct Ed25519Signature {
    inner: ed25519_dalek::Signature,
}

impl Ed25519Signature {
    /// Create from bytes (64 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != lengths::ED25519_SIGNATURE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::ED25519_SIGNATURE,
                actual: bytes.len(),
            });
        }

        let sig = ed25519_dalek::Signature::from_slice(bytes)
            .map_err(|_| CryptoError::InvalidSignature)?;

        Ok(Self { inner: sig })
    }

    /// Get signature bytes
    pub fn to_bytes(&self) -> [u8; 64] {
        self.inner.to_bytes()
    }
}
