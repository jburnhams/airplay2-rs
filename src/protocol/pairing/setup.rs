//! Pair-Setup - PIN-based pairing using SRP-6a
//!
//! This is used when first connecting to a device that requires authentication.
//! The user must enter a PIN displayed on the device.

use super::{
    tlv::{methods, errors, TlvDecoder, TlvEncoder, TlvType},
    PairingError, PairingState, PairingStepResult, SessionKeys,
};
use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce, SrpClient, SrpVerifier,
};

/// Pair-Setup session for PIN-based pairing
pub struct PairSetup {
    state: PairingState,
    /// PIN entered by user
    pin: Option<String>,
    /// SRP verifier (stores state between M2 and M4)
    srp_verifier: Option<SrpVerifier>,
    /// Our Ed25519 long-term key pair
    signing_keypair: Ed25519KeyPair,
    /// Session key from SRP
    session_key: Option<Vec<u8>>,
    /// Device's Ed25519 public key (for verification)
    device_ltpk: Option<Vec<u8>>,
}

impl PairSetup {
    /// Create a new Pair-Setup session
    ///
    /// # Errors
    /// Returns `PairingError` if key generation fails (should typically not happen).
    pub fn new() -> Result<Self, PairingError> {
        let signing_keypair = Ed25519KeyPair::generate();

        Ok(Self {
            state: PairingState::Init,
            pin: None,
            srp_verifier: None,
            signing_keypair,
            session_key: None,
            device_ltpk: None,
        })
    }

    /// Set the PIN for authentication
    pub fn set_pin(&mut self, pin: &str) {
        self.pin = Some(pin.to_string());
    }

    /// Start pairing - returns M1 message
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

        // Build M1: state=1, method=0 (pair-setup)
        let m1 = TlvEncoder::new()
            .add_state(1)
            .add_method(methods::PAIR_SETUP)
            .build();

        self.state = PairingState::WaitingResponse;
        Ok(m1)
    }

    /// Process M2 (salt + server public key) and generate M3
    ///
    /// # Errors
    /// Returns `PairingError` if the state is invalid, PIN is not set, or processing fails.
    pub fn process_m2(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        let tlv = TlvDecoder::decode(data)?;

        // Check for error
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

        // Get salt and server public key
        let salt = tlv.get_required(TlvType::Salt)?;
        let server_public = tlv.get_required(TlvType::PublicKey)?;

        // Get PIN (must be set before this step)
        let pin = self.pin.as_ref().ok_or(PairingError::AuthenticationFailed(
            "PIN not set".to_string(),
        ))?;

        // Create SRP client and process challenge
        let srp_client = SrpClient::new()?;
        let client_public = srp_client.public_key().to_vec();

        let verifier =
            srp_client.process_challenge(b"Pair-Setup", pin.as_bytes(), salt, server_public)?;

        // Build M3: state=3, public_key=A, proof=M1
        let m3 = TlvEncoder::new()
            .add_state(3)
            .add(TlvType::PublicKey, &client_public)
            .add(TlvType::Proof, verifier.client_proof())
            .build();

        self.srp_verifier = Some(verifier);
        self.state = PairingState::SrpExchange;

        Ok(PairingStepResult::SendData(m3))
    }

    /// Process M4 (server proof) and generate M5
    ///
    /// # Errors
    /// Returns `PairingError` if the state is invalid or processing fails.
    pub fn process_m4(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        let tlv = TlvDecoder::decode(data)?;

        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            if error == errors::AUTHENTICATION {
                return Err(PairingError::SrpVerificationFailed);
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

        // Verify server proof
        let server_proof = tlv.get_required(TlvType::Proof)?;

        // Verify server proof with SRP verifier
        let srp_verifier = self
            .srp_verifier
            .as_ref()
            .ok_or(PairingError::InvalidState {
                expected: "SrpVerifier".to_string(),
                actual: "None".to_string(),
            })?;

        let srp_session_key = srp_verifier
            .verify_server(server_proof)
            .map_err(|_| PairingError::SrpVerificationFailed)?;

        // Derive session key using HKDF
        let session_key = srp_session_key.as_bytes().to_vec();

        // Derive encryption key for M5
        let hkdf = HkdfSha512::new(Some(b"Pair-Setup-Encrypt-Salt"), &session_key);
        let encrypt_key = hkdf.expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")?;

        // Build inner TLV with our identifier and public key
        // No unused variable warning needed because we encode it then sign it separately
        let _inner_tlv_encoder = TlvEncoder::new()
            .add(TlvType::Identifier, b"airplay2-rs")
            .add(
                TlvType::PublicKey,
                self.signing_keypair.public_key().as_bytes(),
            );

        // Sign: HKDF(...) || identifier || public_key
        let mut sign_data = hkdf.expand(b"Pair-Setup-Controller-Sign-Salt", 32)?;
        sign_data.extend_from_slice(b"airplay2-rs");
        sign_data.extend_from_slice(self.signing_keypair.public_key().as_bytes());

        let signature = self.signing_keypair.sign(&sign_data);

        let signed_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, b"airplay2-rs")
            .add(
                TlvType::PublicKey,
                self.signing_keypair.public_key().as_bytes(),
            )
            .add(TlvType::Signature, &signature.to_bytes())
            .build();

        // Encrypt the signed TLV
        let cipher = ChaCha20Poly1305Cipher::new(&encrypt_key)?;
        let nonce = Nonce::from_bytes(&[0u8; 12])?;
        let encrypted = cipher.encrypt(&nonce, &signed_tlv)?;

        // Build M5
        let m5 = TlvEncoder::new()
            .add_state(5)
            .add(TlvType::EncryptedData, &encrypted)
            .build();

        self.session_key = Some(session_key);
        self.state = PairingState::KeyExchange;

        Ok(PairingStepResult::SendData(m5))
    }

    /// Process M6 (device info) - completes pairing
    ///
    /// # Errors
    /// Returns `PairingError` if the state is invalid or processing fails.
    pub fn process_m6(&mut self, data: &[u8]) -> Result<PairingStepResult, PairingError> {
        let tlv = TlvDecoder::decode(data)?;

        if let Some(error) = tlv.get_error() {
            self.state = PairingState::Failed;
            return Err(PairingError::DeviceError { code: error });
        }

        let state = tlv.get_state()?;
        if state != 6 {
            return Err(PairingError::InvalidState {
                expected: "6".to_string(),
                actual: state.to_string(),
            });
        }

        // Decrypt device info
        let encrypted = tlv.get_required(TlvType::EncryptedData)?;

        let session_key = self
            .session_key
            .as_ref()
            .ok_or(PairingError::InvalidState {
                expected: "session_key".to_string(),
                actual: "none".to_string(),
            })?;

        let hkdf = HkdfSha512::new(Some(b"Pair-Setup-Encrypt-Salt"), session_key);
        let decrypt_key = hkdf.expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")?;

        let cipher = ChaCha20Poly1305Cipher::new(&decrypt_key)?;
        let nonce = Nonce::from_bytes(&[0u8; 12])?;
        let decrypted = cipher.decrypt(&nonce, encrypted)?;

        // Parse device info TLV
        let device_tlv = TlvDecoder::decode(&decrypted)?;
        let device_ltpk = device_tlv.get_required(TlvType::PublicKey)?.to_vec();

        // TODO: Verify device signature
        // Note: Real implementation should verify device signature here using device_ltpk

        self.device_ltpk = Some(device_ltpk);
        self.state = PairingState::Complete;

        // Derive final session keys
        let encrypt_key = hkdf.expand_fixed::<32>(b"Control-Write-Encryption-Key")?;
        let decrypt_key = hkdf.expand_fixed::<32>(b"Control-Read-Encryption-Key")?;

        let session_keys = SessionKeys {
            encrypt_key,
            decrypt_key,
            encrypt_nonce: 0,
            decrypt_nonce: 0,
        };

        Ok(PairingStepResult::Complete(session_keys))
    }

    /// Get our long-term public key (for storage)
    #[must_use]
    pub fn our_public_key(&self) -> [u8; 32] {
        *self.signing_keypair.public_key().as_bytes()
    }

    /// Get our long-term secret key (for storage)
    #[must_use]
    pub fn our_secret_key(&self) -> [u8; 32] {
        self.signing_keypair.secret_bytes()
    }

    /// Get device's long-term public key (for storage)
    #[must_use]
    pub fn device_public_key(&self) -> Option<&[u8]> {
        self.device_ltpk.as_deref()
    }
}
