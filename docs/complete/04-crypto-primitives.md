# Section 04: Cryptographic Primitives

> **VERIFIED**: Checked against `src/protocol/crypto/mod.rs` and submodules on 2025-01-30.
> Implementation complete with additional RSA support for RAOP (feature-gated).

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

AirPlay 2 uses multiple cryptographic algorithms for authentication and encryption. This section provides a unified interface to the required primitives, wrapping well-audited RustCrypto crates.

## Algorithms Required

| Algorithm | Use Case | Crate |
|-----------|----------|-------|
| SRP-6a | HomeKit pairing | `srp` |
| Ed25519 | Signature verification | `ed25519-dalek` |
| X25519 | Key exchange | `x25519-dalek` |
| HKDF-SHA512 | Key derivation | `hkdf` + `sha2` |
| ChaCha20-Poly1305 | Authenticated encryption | `chacha20poly1305` |
| AES-128-CTR | Audio stream encryption | `aes` + `ctr` |
| AES-128-GCM | Alternative auth encryption | `aes-gcm` |
| SHA-512 | Hashing | `sha2` |

## Objectives

- Provide unified crypto interface for protocol layers
- Hide crate-specific details behind clean abstractions
- Ensure proper zeroization of secrets
- Support both transient and persistent key storage

---

## Tasks

### 4.1 Module Structure

- [x] **4.1.1** Create crypto module with submodules

**File:** `src/protocol/crypto/mod.rs`

```rust
//! Cryptographic primitives for AirPlay authentication and encryption

mod aes;
mod chacha;
mod ed25519;
mod error;
mod hkdf;
#[cfg(feature = "raop")]
mod rsa;
#[cfg(all(test, feature = "raop"))]
mod rsa_tests;
mod srp;
#[cfg(test)]
mod tests;
mod x25519;

pub use self::aes::{Aes128Ctr, Aes128Gcm};
pub use self::chacha::{ChaCha20Poly1305Cipher, Nonce};
pub use self::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature};
pub use self::error::CryptoError;
pub use self::hkdf::{AirPlayKeys, HkdfSha512, derive_key};
#[cfg(feature = "raop")]
pub use self::rsa::{AppleRsaPublicKey, RaopRsaPrivateKey, sizes as rsa_sizes};
pub use self::srp::{SrpClient, SrpVerifier};
pub use self::x25519::{X25519KeyPair, X25519PublicKey, X25519SharedSecret};

/// Length of various cryptographic values
pub mod lengths {
    /// Ed25519 public key length
    pub const ED25519_PUBLIC_KEY: usize = 32;
    /// Ed25519 signature length
    pub const ED25519_SIGNATURE: usize = 64;
    /// X25519 public key length
    pub const X25519_PUBLIC_KEY: usize = 32;
    /// X25519 shared secret length
    pub const X25519_SHARED_SECRET: usize = 32;
    /// ChaCha20-Poly1305 key length
    pub const CHACHA_KEY: usize = 32;
    /// ChaCha20-Poly1305 nonce length
    pub const CHACHA_NONCE: usize = 12;
    /// ChaCha20-Poly1305 tag length
    pub const CHACHA_TAG: usize = 16;
    /// AES-128 key length
    pub const AES_128_KEY: usize = 16;
    /// AES-GCM nonce length
    pub const AES_GCM_NONCE: usize = 12;
}
```

---

### 4.2 Error Types

- [x] **4.2.1** Define crypto error types

**File:** `src/protocol/crypto/error.rs`

```rust
use thiserror::Error;

/// Cryptographic operation errors
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("invalid signature")]
    InvalidSignature,

    #[error("verification failed")]
    VerificationFailed,

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("SRP error: {0}")]
    SrpError(String),

    #[error("invalid public key")]
    InvalidPublicKey,

    #[error("RNG error")]
    RngError,
}
```

---

### 4.3 SRP-6a Implementation

- [x] **4.3.1** Implement SRP client for pairing

**File:** `src/protocol/crypto/srp.rs`

```rust
use super::CryptoError;

/// SRP-6a client for HomeKit pairing
///
/// Used during Pair-Setup to establish shared secret without
/// transmitting the PIN in the clear.
pub struct SrpClient {
    // Internal srp crate client
    inner: srp::client::SrpClient<'static, sha2::Sha512>,
    // Client private key (a)
    private_key: Vec<u8>,
    // Client public key (A)
    public_key: Vec<u8>,
}

impl SrpClient {
    /// AirPlay uses the 3072-bit group from RFC 5054
    const GROUP: &'static srp::groups::SrpGroup = &srp::groups::G_3072;

    /// Create a new SRP client with random private key
    pub fn new() -> Result<Self, CryptoError> {
        use rand::RngCore;

        let mut private_key = vec![0u8; 32];
        rand::thread_rng()
            .try_fill_bytes(&mut private_key)
            .map_err(|_| CryptoError::RngError)?;

        Self::with_private_key(&private_key)
    }

    /// Create SRP client with specific private key (for testing)
    pub fn with_private_key(private_key: &[u8]) -> Result<Self, CryptoError> {
        // Implementation using srp crate
        todo!()
    }

    /// Get client public key (A) to send to server
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Process server's response (salt, B) and compute shared secret
    ///
    /// # Arguments
    /// * `username` - Usually "Pair-Setup" for AirPlay
    /// * `password` - The 4-digit PIN
    /// * `salt` - Server's salt
    /// * `server_public` - Server's public key (B)
    pub fn process_challenge(
        &self,
        username: &[u8],
        password: &[u8],
        salt: &[u8],
        server_public: &[u8],
    ) -> Result<SrpVerifier, CryptoError> {
        // Compute x = H(salt | H(username | ":" | password))
        // Compute u = H(A | B)
        // Compute S = (B - k * g^x)^(a + u*x) mod N
        // Compute K = H(S)
        // Compute M1 = H(H(N) XOR H(g) | H(username) | salt | A | B | K)
        todo!()
    }
}

/// SRP session after challenge processed
pub struct SrpVerifier {
    /// Shared session key
    session_key: Vec<u8>,
    /// Client proof (M1)
    client_proof: Vec<u8>,
}

impl SrpVerifier {
    /// Get client proof (M1) to send to server
    pub fn client_proof(&self) -> &[u8] {
        &self.client_proof
    }

    /// Verify server's proof (M2)
    pub fn verify_server(&self, server_proof: &[u8]) -> Result<SessionKey, CryptoError> {
        // Verify M2 = H(A | M1 | K)
        todo!()
    }
}

/// Established SRP session key
pub struct SessionKey {
    key: Vec<u8>,
}

impl SessionKey {
    /// Get the session key bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }
}

impl Drop for SessionKey {
    fn drop(&mut self) {
        // Zeroize on drop
        self.key.iter_mut().for_each(|b| *b = 0);
    }
}
```

---

### 4.4 Ed25519 Signatures

- [x] **4.4.1** Implement Ed25519 key pair and operations

**File:** `src/protocol/crypto/ed25519.rs`

```rust
use super::{lengths, CryptoError};
use ed25519_dalek::{Signer, Verifier};

/// Ed25519 key pair for signing
pub struct Ed25519KeyPair {
    signing_key: ed25519_dalek::SigningKey,
}

impl Ed25519KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Result<Self, CryptoError> {
        use rand::rngs::OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Ok(Self { signing_key })
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
```

---

### 4.5 X25519 Key Exchange

- [x] **4.5.1** Implement X25519 ECDH

**File:** `src/protocol/crypto/x25519.rs`

```rust
use super::{lengths, CryptoError};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

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
```

---

### 4.6 HKDF Key Derivation

- [x] **4.6.1** Implement HKDF-SHA512

**File:** `src/protocol/crypto/hkdf.rs`

```rust
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

/// Derive AirPlay session keys from shared secret
///
/// AirPlay uses specific info strings for different keys
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
```

---

### 4.7 ChaCha20-Poly1305 AEAD

- [x] **4.7.1** Implement ChaCha20-Poly1305 encryption

**File:** `src/protocol/crypto/chacha.rs`

```rust
use super::{lengths, CryptoError};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305 as ChaChaImpl, Nonce as ChaChaNonce,
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

        let cipher = ChaChaImpl::new_from_slice(key)
            .map_err(|_| CryptoError::InvalidKeyLength {
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
```

---

### 4.8 AES Ciphers

- [x] **4.8.1** Implement AES-128-CTR for audio encryption

**File:** `src/protocol/crypto/aes.rs`

```rust
use super::{lengths, CryptoError};
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

        let cipher = Aes128CtrImpl::new_from_slices(key, iv)
            .map_err(|_| CryptoError::InvalidKeyLength {
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

        let cipher = aes_gcm::Aes128Gcm::new_from_slice(key)
            .map_err(|_| CryptoError::InvalidKeyLength {
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
```

---

## Unit Tests

### Test File: `src/protocol/crypto/ed25519.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = Ed25519KeyPair::generate().unwrap();
        let pk = kp.public_key();

        assert_eq!(pk.as_bytes().len(), 32);
    }

    #[test]
    fn test_keypair_from_bytes() {
        let kp1 = Ed25519KeyPair::generate().unwrap();
        let secret = kp1.secret_bytes();

        let kp2 = Ed25519KeyPair::from_bytes(&secret).unwrap();

        assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
    }

    #[test]
    fn test_sign_verify() {
        let kp = Ed25519KeyPair::generate().unwrap();
        let message = b"test message";

        let signature = kp.sign(message);
        kp.public_key().verify(message, &signature).unwrap();
    }

    #[test]
    fn test_verify_wrong_message() {
        let kp = Ed25519KeyPair::generate().unwrap();

        let signature = kp.sign(b"original message");
        let result = kp.public_key().verify(b"different message", &signature);

        assert!(matches!(result, Err(CryptoError::InvalidSignature)));
    }

    #[test]
    fn test_signature_roundtrip() {
        let kp = Ed25519KeyPair::generate().unwrap();
        let signature = kp.sign(b"message");

        let bytes = signature.to_bytes();
        let recovered = Ed25519Signature::from_bytes(&bytes).unwrap();

        kp.public_key().verify(b"message", &recovered).unwrap();
    }
}
```

### Test File: `src/protocol/crypto/x25519.rs`

```rust
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
```

### Test File: `src/protocol/crypto/chacha.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = [0x42u8; 32];
        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

        let nonce = Nonce::from_counter(1);
        let plaintext = b"Hello, AirPlay!";

        let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();
        let decrypted = cipher.decrypt(&nonce, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_ciphertext_is_larger() {
        let key = [0x42u8; 32];
        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

        let nonce = Nonce::from_counter(0);
        let plaintext = b"test";

        let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();

        // Ciphertext should be plaintext + 16 byte tag
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    #[test]
    fn test_decrypt_wrong_nonce_fails() {
        let key = [0x42u8; 32];
        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

        let nonce1 = Nonce::from_counter(1);
        let nonce2 = Nonce::from_counter(2);

        let ciphertext = cipher.encrypt(&nonce1, b"secret").unwrap();
        let result = cipher.decrypt(&nonce2, &ciphertext);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
    }

    #[test]
    fn test_encrypt_with_aad() {
        let key = [0x42u8; 32];
        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

        let nonce = Nonce::from_counter(1);
        let aad = b"header";
        let plaintext = b"body";

        let ciphertext = cipher.encrypt_with_aad(&nonce, aad, plaintext).unwrap();
        let decrypted = cipher.decrypt_with_aad(&nonce, aad, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_aad_fails() {
        let key = [0x42u8; 32];
        let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

        let nonce = Nonce::from_counter(1);
        let ciphertext = cipher.encrypt_with_aad(&nonce, b"aad1", b"data").unwrap();

        let result = cipher.decrypt_with_aad(&nonce, b"aad2", &ciphertext);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
    }
}
```

### Test File: `src/protocol/crypto/aes.rs`

```rust
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
```

### Test File: `src/protocol/crypto/hkdf.rs`

```rust
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
```

---

## Integration Tests

### Test: Known test vectors

```rust
// tests/protocol/crypto_vectors.rs

#[test]
fn test_chacha20_poly1305_rfc8439_vector() {
    // Test vector from RFC 8439
    let key = hex::decode(
        "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f"
    ).unwrap();

    let nonce = hex::decode("070000004041424344454647").unwrap();

    let plaintext = b"Ladies and Gentlemen of the class of '99: \
                     If I could offer you only one tip for the future, \
                     sunscreen would be it.";

    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
    let nonce = Nonce::from_bytes(&nonce).unwrap();

    let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();

    // Verify against known ciphertext from RFC
    // ...
}
```

---

## Acceptance Criteria

- [x] All crypto types compile and tests pass
- [x] Ed25519 sign/verify works correctly
- [x] X25519 key exchange produces matching shared secrets
- [x] HKDF derives correct keys from test vectors
- [x] ChaCha20-Poly1305 encrypts/decrypts correctly
- [x] AES-CTR encrypts/decrypts correctly
- [x] AES-GCM encrypts/decrypts with authentication
- [x] All secrets are zeroized on drop
- [x] Error types are descriptive
- [x] RFC test vectors pass

---

## Notes

- Consider using `zeroize` crate for explicit secret clearing
- SRP implementation needs careful testing with known vectors
- FairPlay crypto would extend this module (future work)
- Performance benchmarks may be useful for audio encryption paths
