//! Transient pairing - quick pairing without stored keys
//!
//! This is the simplest pairing method, used when:
//! - Device allows unauthenticated connections
//! - We don't need to store keys for later

use super::{
    tlv::{TlvDecoder, TlvEncoder, TlvType},
    PairingError, PairingState, PairingStepResult, SessionKeys,
};
use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce, X25519KeyPair, X25519PublicKey,
};

/// Transient pairing session
pub struct TransientPairing {
    state: PairingState,
    /// Our X25519 key pair
    our_keypair: X25519KeyPair,
    /// Our Ed25519 key pair for signing
    signing_keypair: Ed25519KeyPair,
    /// Device's public key (received in step 2)
    device_public: Option<X25519PublicKey>,
    /// Shared secret
    shared_secret: Option<[u8; 32]>,
    /// Session keys derived from shared secret
    session_keys: Option<SessionKeys>,
}

impl TransientPairing {
    /// Create a new transient pairing session
    ///
    /// # Errors
    ///
    /// Returns error if key generation fails
    pub fn new() -> Result<Self, PairingError> {
        let our_keypair = X25519KeyPair::generate();
        let signing_keypair = Ed25519KeyPair::generate();

        Ok(Self {
            state: PairingState::Init,
            our_keypair,
            signing_keypair,
            device_public: None,
            shared_secret: None,
            session_keys: None,
        })
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> PairingState {
        self.state
    }

    /// Start pairing - returns M1 message
    ///
    /// # Errors
    ///
    /// Returns error if state is invalid
    pub fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if self.state != PairingState::Init {
            return Err(PairingError::InvalidState {
                expected: "Init".to_string(),
                actual: format!("{:?}", self.state),
            });
        }

        // Build M1: state=1, public key, method=0 (transient)
        let m1 = TlvEncoder::new()
            .add_state(1)
            .add_byte(TlvType::Method, 0)
            .add(TlvType::PublicKey, self.our_keypair.public_key().as_bytes())
            .build();

        self.state = PairingState::WaitingResponse;
        Ok(m1)
    }

    /// Process device response (M2) and generate M3
    ///
    /// # Errors
    ///
    /// Returns error if parsing or processing fails
    pub fn process_m2(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        if self.state != PairingState::WaitingResponse {
            return Err(PairingError::InvalidState {
                expected: "WaitingResponse".to_string(),
                actual: format!("{:?}", self.state),
            });
        }

        let tlv = TlvDecoder::decode(data)?;

        // Check for error
        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            return Err(PairingError::DeviceError { code: error });
        }

        // Verify state
        let state = tlv.get_state()?;
        if state != 2 {
            return Err(PairingError::InvalidState {
                expected: "2".to_string(),
                actual: state.to_string(),
            });
        }

        // Extract device public key
        let device_pub_bytes = tlv.get_required(TlvType::PublicKey)?;
        let device_public = X25519PublicKey::from_bytes(device_pub_bytes)?;

        // Compute shared secret
        let shared_secret = self.our_keypair.diffie_hellman(&device_public);

        // Derive session keys using HKDF
        let hkdf = HkdfSha512::new(
            Some(b"Pair-Verify-Encrypt-Salt"),
            shared_secret.as_bytes(),
        );

        let session_key = hkdf.expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")?;

        // Create proof by signing: our_public || device_public
        let mut proof_data = Vec::new();
        proof_data.extend_from_slice(self.our_keypair.public_key().as_bytes());
        proof_data.extend_from_slice(device_pub_bytes);

        let signature = self.signing_keypair.sign(&proof_data);

        // Encrypt our identifier and signature
        let inner_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, b"airplay2-rs")
            .add(TlvType::Signature, &signature.to_bytes())
            .build();

        let cipher = ChaCha20Poly1305Cipher::new(&session_key)?;
        let nonce = Nonce::from_bytes(&[0u8; 12])?;
        let encrypted = cipher.encrypt(&nonce, &inner_tlv)?;

        // Build M3
        let m3 = TlvEncoder::new()
            .add_state(3)
            .add(TlvType::EncryptedData, &encrypted)
            .build();

        // Store state
        self.device_public = Some(device_public);
        self.shared_secret = Some(*shared_secret.as_bytes());
        self.state = PairingState::Verifying;

        Ok(PairingStepResult::SendData(m3))
    }

    /// Process device response (M4) - completes pairing
    ///
    /// # Errors
    ///
    /// Returns error if parsing or processing fails
    pub fn process_m4(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        if self.state != PairingState::Verifying {
            return Err(PairingError::InvalidState {
                expected: "Verifying".to_string(),
                actual: format!("{:?}", self.state),
            });
        }

        let tlv = TlvDecoder::decode(data)?;

        // Check for error
        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            return Err(PairingError::DeviceError { code: error });
        }

        // Verify state
        let state = tlv.get_state()?;
        if state != 4 {
            return Err(PairingError::InvalidState {
                expected: "4".to_string(),
                actual: state.to_string(),
            });
        }

        // Derive final session keys
        let shared_secret = self.shared_secret.as_ref().ok_or(PairingError::InvalidState {
            expected: "shared_secret set".to_string(),
            actual: "none".to_string(),
        })?;

        let hkdf = HkdfSha512::new(Some(b"Control-Salt"), shared_secret);

        let encrypt_key = hkdf.expand_fixed::<32>(b"Control-Write-Encryption-Key")?;
        let decrypt_key = hkdf.expand_fixed::<32>(b"Control-Read-Encryption-Key")?;

        let session_keys = SessionKeys {
            encrypt_key,
            decrypt_key,
            encrypt_nonce: 0,
            decrypt_nonce: 0,
        };

        self.session_keys = Some(session_keys.clone());
        self.state = PairingState::Complete;

        Ok(PairingStepResult::Complete(session_keys))
    }

    /// Drive the pairing state machine with received data
    ///
    /// # Errors
    ///
    /// Returns error if processing fails or state transition is invalid
    ///
    /// # Panics
    ///
    /// Panics if state is `Complete` but session keys are missing (should not happen)
    pub fn step(&mut self, data: Option<&[u8]>) -> Result<PairingStepResult, PairingError> {
        match self.state {
            PairingState::Init => {
                let m1 = self.start()?;
                Ok(PairingStepResult::SendData(m1))
            }
            PairingState::WaitingResponse => {
                let data = data.ok_or(PairingError::InvalidState {
                    expected: "data".to_string(),
                    actual: "none".to_string(),
                })?;
                self.process_m2(data)
            }
            PairingState::Verifying => {
                let data = data.ok_or(PairingError::InvalidState {
                    expected: "data".to_string(),
                    actual: "none".to_string(),
                })?;
                self.process_m4(data)
            }
            PairingState::Complete => Ok(PairingStepResult::Complete(
                self.session_keys.clone().unwrap(),
            )),
            PairingState::Failed => Err(PairingError::InvalidState {
                expected: "not failed".to_string(),
                actual: "Failed".to_string(),
            }),
            _ => Ok(PairingStepResult::NeedData),
        }
    }
}

impl Default for TransientPairing {
    fn default() -> Self {
        Self::new().expect("failed to create transient pairing")
    }
}
