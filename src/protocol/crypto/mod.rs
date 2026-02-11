//! Cryptographic primitives for `AirPlay` authentication and encryption

#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(missing_docs)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]

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
#[cfg(test)]
mod vectors_tests;
mod x25519;

pub use self::aes::{Aes128Ctr, Aes128Gcm};
pub use self::chacha::{ChaCha20Poly1305Cipher, Nonce};
pub use self::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature};
pub use self::error::CryptoError;
pub use self::hkdf::{AirPlayKeys, HkdfSha512, derive_key};
#[cfg(feature = "raop")]
pub use self::rsa::{AppleRsaPublicKey, CompatibleOsRng, RaopRsaPrivateKey, sizes as rsa_sizes};
pub use self::srp::{SessionKey, SrpClient, SrpGroup, SrpParams, SrpServer, SrpVerifier};
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
