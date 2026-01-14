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
