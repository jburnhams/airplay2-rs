//! Pair-Verify - Fast verification using stored keys
//!
//! Used after initial Pair-Setup to quickly establish a session
//! without requiring PIN entry again.

use super::tlv::{TlvDecoder, TlvEncoder, TlvType, errors};
use super::{PairingError, PairingKeys, PairingState, PairingStepResult, SessionKeys};
use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature, HkdfSha512, Nonce,
    X25519KeyPair, X25519PublicKey,
};

/// Pair-Verify session
pub struct PairVerify {
    state: PairingState,
    /// Our stored keys
    our_keys: PairingKeys,
    /// Device's stored public key
    device_ltpk: Ed25519PublicKey,
    /// Ephemeral X25519 key pair for this session
    ephemeral_keypair: X25519KeyPair,
    /// Device's ephemeral public key
    device_ephemeral: Option<X25519PublicKey>,
    /// Shared secret from ephemeral exchange
    shared_secret: Option<[u8; 32]>,
    /// Session encryption key
    session_key: Option<[u8; 32]>,
    /// Final session keys (stored after completion)
    final_session_keys: Option<SessionKeys>,
}

impl PairVerify {
    /// Create a new Pair-Verify session with stored keys
    ///
    /// # Errors
    ///
    /// Returns error if key format is invalid
    pub fn new(our_keys: PairingKeys, device_ltpk: &[u8]) -> Result<Self, PairingError> {
        let device_ltpk = Ed25519PublicKey::from_bytes(device_ltpk)?;
        let ephemeral_keypair = X25519KeyPair::generate();

        Ok(Self {
            state: PairingState::Init,
            our_keys,
            device_ltpk,
            ephemeral_keypair,
            device_ephemeral: None,
            shared_secret: None,
            session_key: None,
            final_session_keys: None,
        })
    }

    /// Start verification - returns M1 message
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

        // Build M1: state=1, public_key=ephemeral
        let m1 = TlvEncoder::new()
            .add_state(1)
            .add(
                TlvType::PublicKey,
                self.ephemeral_keypair.public_key().as_bytes(),
            )
            .build();

        self.state = PairingState::WaitingResponse;
        Ok(m1)
    }

    /// Process M2 and generate M3
    ///
    /// # Errors
    ///
    /// Returns error if processing fails
    pub fn process_m2(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        let tlv = TlvDecoder::decode(data)?;

        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            return Err(PairingError::DeviceError { code: error });
        }

        let state = tlv.get_state()?;
        if state != 2 {
            return Err(PairingError::InvalidState {
                expected: "2".to_string(),
                actual: state.to_string(),
            });
        }

        // Get device's ephemeral public key and encrypted data
        let device_ephemeral_bytes = tlv.get_required(TlvType::PublicKey)?;
        let encrypted_data = tlv.get_required(TlvType::EncryptedData)?;

        let device_ephemeral = X25519PublicKey::from_bytes(device_ephemeral_bytes)?;

        // Compute shared secret
        let shared = self.ephemeral_keypair.diffie_hellman(&device_ephemeral);

        // Derive session key
        let hkdf = HkdfSha512::new(Some(b"Pair-Verify-Encrypt-Salt"), shared.as_bytes());
        let session_key = hkdf.expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")?;

        // Decrypt device's signature
        let cipher = ChaCha20Poly1305Cipher::new(&session_key)?;

        // Use "PV-Msg02" as nonce (padded to 12 bytes with prefix zeros)
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(b"PV-Msg02");
        let nonce = Nonce::from_bytes(&nonce_bytes)?;

        let decrypted = cipher.decrypt(&nonce, encrypted_data)?;

        let device_tlv = TlvDecoder::decode(&decrypted)?;
        let _device_identifier = device_tlv.get_required(TlvType::Identifier)?;
        let device_signature = device_tlv.get_required(TlvType::Signature)?;

        // Verify device's signature: device_ephemeral || our_ephemeral
        let mut verify_data = Vec::new();
        verify_data.extend_from_slice(device_ephemeral_bytes);
        verify_data.extend_from_slice(self.ephemeral_keypair.public_key().as_bytes());

        let signature = Ed25519Signature::from_bytes(device_signature)?;
        self.device_ltpk.verify(&verify_data, &signature)?;

        // Create our signature: our_ephemeral || device_ephemeral
        let mut sign_data = Vec::new();
        sign_data.extend_from_slice(self.ephemeral_keypair.public_key().as_bytes());
        sign_data.extend_from_slice(device_ephemeral_bytes);

        let our_keypair = Ed25519KeyPair::from_bytes(&self.our_keys.secret_key)?;
        let our_signature = our_keypair.sign(&sign_data);

        // Encrypt our response
        let inner_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, &self.our_keys.identifier)
            .add(TlvType::Signature, &our_signature.to_bytes())
            .build();

        // Use "PV-Msg03" as nonce (padded to 12 bytes with prefix zeros)
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(b"PV-Msg03");
        let nonce = Nonce::from_bytes(&nonce_bytes)?;

        let encrypted = cipher.encrypt(&nonce, &inner_tlv)?;

        // Build M3
        let m3 = TlvEncoder::new()
            .add_state(3)
            .add(TlvType::EncryptedData, &encrypted)
            .build();

        self.device_ephemeral = Some(device_ephemeral);
        self.shared_secret = Some(*shared.as_bytes());
        self.session_key = Some(session_key);
        self.state = PairingState::Verifying;

        Ok(PairingStepResult::SendData(m3))
    }

    /// Process M4 - completes verification
    ///
    /// # Errors
    ///
    /// Returns error if processing fails
    pub fn process_m4(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        let tlv = TlvDecoder::decode(data)?;

        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            if error == errors::AUTHENTICATION {
                return Err(PairingError::SignatureVerificationFailed);
            }
            return Err(PairingError::DeviceError { code: error });
        }

        let state = tlv.get_state()?;
        if state != 4 {
            return Err(PairingError::InvalidState {
                expected: "4".to_string(),
                actual: state.to_string(),
            });
        }

        // Derive final session keys
        let shared_secret = self
            .shared_secret
            .as_ref()
            .ok_or(PairingError::InvalidState {
                expected: "shared_secret".to_string(),
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
            raw_shared_secret: *shared_secret,
        };

        self.final_session_keys = Some(session_keys.clone());
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
                self.final_session_keys.clone().unwrap(),
            )),
            PairingState::Failed => Err(PairingError::InvalidState {
                expected: "not failed".to_string(),
                actual: "Failed".to_string(),
            }),
            _ => Ok(PairingStepResult::NeedData),
        }
    }
}
