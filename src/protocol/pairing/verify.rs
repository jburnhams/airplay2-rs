//! Pair-Verify - Fast verification using stored keys
//!
//! Used after initial Pair-Setup to quickly establish a session
//! without requiring PIN entry again.

use super::{
    tlv::{errors, TlvDecoder, TlvEncoder, TlvType},
    PairingError, PairingKeys, PairingState, PairingStepResult, SessionKeys,
};
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
    #[allow(dead_code)] // Stored but potentially used for next steps or debugging
    session_key: Option<[u8; 32]>,
}

impl PairVerify {
    /// Create a new Pair-Verify session with stored keys
    ///
    /// # Errors
    /// Returns `PairingError` if the device public key is invalid.
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
        })
    }

    /// Start verification - returns M1 message
    ///
    /// # Errors
    /// Returns `PairingError` if the state is invalid.
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
    /// Returns `PairingError` if the state is invalid or processing fails.
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
        let nonce = Nonce::from_bytes(&[0u8; 12])?;
        let decrypted = cipher.decrypt(&nonce, encrypted_data)?;

        let device_tlv = TlvDecoder::decode(&decrypted)?;
        let _device_identifier = device_tlv.get_required(TlvType::Identifier)?; // Unused for now
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

        // Note: Nonce logic here assumes specific protocol details.
        // Usually nonce increments, but here we reuse cipher or need new nonce?
        // The markdown says "nonce = ... 0...01".
        // Let's check `ChaCha20Poly1305Cipher` if it allows specifying nonce on encrypt.
        // It does: `encrypt(&nonce, ...)`
        // So we create a new cipher instance (same key) or reuse key.
        // We already have `cipher` variable.
        // But `cipher` was created with `nonce_counter` implicitly? No, `ChaCha20Poly1305Cipher::new` just sets key.
        // `encrypt` takes `&Nonce`.

        let nonce = Nonce::from_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])?;
        // We can reuse the `cipher` variable as it holds the key state (immutable mostly or cloned?)
        // `ChaCha20Poly1305Cipher` is a struct wrapper around `ChaCha20Poly1305` which is stateless + key.
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
    /// Returns `PairingError` if the state is invalid or processing fails.
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
        };

        self.state = PairingState::Complete;

        Ok(PairingStepResult::Complete(session_keys))
    }
}
