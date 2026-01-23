//! Auth-Setup - `MFi` authentication handshake
//!
//! This step is required by `AirPlay` 2 devices, even if we don't perform full `MFi` verification.
//! It establishes an ephemeral Curve25519 shared secret.

use super::PairingError;
use crate::protocol::crypto::X25519KeyPair;

/// Auth-Setup session
pub struct AuthSetup {
    /// Our Curve25519 key pair
    keypair: X25519KeyPair,
}

impl Default for AuthSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthSetup {
    /// Create a new Auth-Setup session
    #[must_use]
    pub fn new() -> Self {
        Self {
            keypair: X25519KeyPair::generate(),
        }
    }

    /// Start auth setup - returns request body
    ///
    /// Format: <1:Encryption Type> <32:Client’s Curve25119 public key>
    /// Encryption Type: 0x01 (Unencrypted - i.e. no MFi-SAP)
    #[must_use]
    pub fn start(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(33);
        body.push(0x01); // Unencrypted
        body.extend_from_slice(self.keypair.public_key().as_bytes());
        body
    }

    /// Process response - returns result
    ///
    /// Note: In a real `MFi` implementation, we would verify the signature here.
    /// Since we are open source and don't have `MFi` keys, we just accept it.
    ///
    /// Response Format:
    /// <32:Server’s Curve25119 public key>
    /// <4:Certificate length (int32be)>
    /// <n:PKCS#7 DER encoded `MFiCertificate`>
    /// <4:Signature length (int32be)>
    /// <n:Signature>
    ///
    /// # Errors
    ///
    /// Returns error if response is too short or malformed
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor logic fails (should not happen if length checks pass)
    pub fn process_response(&self, data: &[u8]) -> Result<(), PairingError> {
        if data.len() < 32 {
            return Err(PairingError::AuthenticationFailed(
                "Auth-Setup response too short".to_string(),
            ));
        }

        // We can parse it just to validate structure, even if we don't verify signature
        let mut cursor = 32;

        if data.len() < cursor + 4 {
            return Err(PairingError::AuthenticationFailed(
                "Auth-Setup response missing cert length".to_string(),
            ));
        }

        let cert_len = u32::from_be_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        if data.len() < cursor + cert_len {
            return Err(PairingError::AuthenticationFailed(
                "Auth-Setup response missing certificate data".to_string(),
            ));
        }
        cursor += cert_len;

        if data.len() < cursor + 4 {
            return Err(PairingError::AuthenticationFailed(
                "Auth-Setup response missing signature length".to_string(),
            ));
        }
        let sig_len = u32::from_be_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        if data.len() < cursor + sig_len {
            return Err(PairingError::AuthenticationFailed(
                "Auth-Setup response missing signature data".to_string(),
            ));
        }

        Ok(())
    }
}
