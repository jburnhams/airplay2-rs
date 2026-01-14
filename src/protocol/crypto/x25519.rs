use super::{CryptoError, lengths};
use x25519_dalek::{PublicKey, StaticSecret};

/// X25519 key pair for Diffie-Hellman key exchange
pub struct X25519KeyPair {
    secret: StaticSecret,
    public: PublicKey,
}

impl X25519KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Self {
        use rand::rngs::OsRng;
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Create from secret key bytes (32 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 32,
                actual: bytes.len(),
            });
        }

        let bytes: [u8; 32] = bytes.try_into().unwrap();
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);

        Ok(Self { secret, public })
    }

    /// Get public key
    pub fn public_key(&self) -> X25519PublicKey {
        X25519PublicKey { inner: self.public }
    }

    /// Get secret key bytes (for storage)
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// Perform Diffie-Hellman key exchange
    pub fn diffie_hellman(&self, their_public: &X25519PublicKey) -> X25519SharedSecret {
        let shared = self.secret.diffie_hellman(&their_public.inner);
        X25519SharedSecret {
            bytes: shared.to_bytes(),
        }
    }
}

/// X25519 public key
#[derive(Clone, Copy)]
pub struct X25519PublicKey {
    inner: PublicKey,
}

impl X25519PublicKey {
    /// Create from bytes (32 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != lengths::X25519_PUBLIC_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::X25519_PUBLIC_KEY,
                actual: bytes.len(),
            });
        }

        let bytes: [u8; 32] = bytes.try_into().unwrap();
        Ok(Self {
            inner: PublicKey::from(bytes),
        })
    }

    /// Get public key bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.inner.as_bytes()
    }
}

/// X25519 shared secret from DH exchange
pub struct X25519SharedSecret {
    bytes: [u8; 32],
}

impl X25519SharedSecret {
    /// Get shared secret bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

impl Drop for X25519SharedSecret {
    fn drop(&mut self) {
        // Zeroize on drop
        self.bytes.iter_mut().for_each(|b| *b = 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_exchange() {
        let alice = X25519KeyPair::generate();
        let bob = X25519KeyPair::generate();

        let alice_shared = alice.diffie_hellman(&bob.public_key());
        let bob_shared = bob.diffie_hellman(&alice.public_key());

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_keypair_roundtrip() {
        let kp1 = X25519KeyPair::generate();
        let secret = kp1.secret_bytes();

        let kp2 = X25519KeyPair::from_bytes(&secret).unwrap();

        assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
    }

    #[test]
    fn test_public_key_from_bytes() {
        let kp = X25519KeyPair::generate();
        let pk_bytes = kp.public_key().as_bytes().to_vec();

        let pk = X25519PublicKey::from_bytes(&pk_bytes).unwrap();

        assert_eq!(pk.as_bytes(), kp.public_key().as_bytes());
    }
}
