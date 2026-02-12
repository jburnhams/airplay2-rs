//! `HomeKit` Pairing Server Implementation
//!
//! This module implements the server side of `HomeKit` pairing, used by
//! `AirPlay` 2 receivers to authenticate connecting senders.

use crate::protocol::pairing::tlv::{TlvEncoder, TlvDecoder, TlvType};
use crate::protocol::crypto::{
    SrpParams, SrpServer,
    Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature,
    X25519KeyPair, X25519PublicKey,
    ChaCha20Poly1305Cipher, Nonce,
    derive_key,
};
use rand::RngCore;
use sha2::{Sha512, Digest};
use thiserror::Error;

/// Pairing server state machine
pub struct PairingServer {
    /// Server's Ed25519 identity keypair (persistent)
    identity: Ed25519KeyPair,

    /// SRP verifier (derived from PIN/password)
    srp_verifier: Option<Vec<u8>>,

    /// SRP salt
    srp_salt: [u8; 16],

    /// Current pairing session state
    state: PairingServerState,

    /// SRP server instance (during pair-setup)
    srp_server: Option<SrpServer>,

    /// Session key from SRP (after M4)
    srp_session_key: Option<[u8; 64]>,

    /// X25519 keypair for pair-verify
    verify_keypair: Option<X25519KeyPair>,

    /// Shared secret from pair-verify
    shared_secret: Option<[u8; 32]>,

    /// Encryption keys (after pair-verify)
    encryption_keys: Option<EncryptionKeys>,

    /// Client's Ed25519 public key (after successful pairing)
    client_public_key: Option<[u8; 32]>,

    /// Client's Curve25519 public key (transient, for pair-verify)
    client_curve_public: Option<[u8; 32]>,
}

/// Encryption keys derived after pairing
#[derive(Clone, Debug)]
pub struct EncryptionKeys {
    /// Key for encrypting messages TO client
    pub encrypt_key: [u8; 32],
    /// Key for decrypting messages FROM client
    pub decrypt_key: [u8; 32],
    /// Nonce counter for encryption
    pub encrypt_nonce: u64,
    /// Nonce counter for decryption
    pub decrypt_nonce: u64,
}

/// State of the pairing server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingServerState {
    /// Waiting for M1
    Idle,
    /// M1 received, sent M2, waiting for M3
    WaitingForM3,
    /// M3 received, sent M4, pair-setup complete
    PairSetupComplete,
    /// Pair-verify M1 received, sent M2, waiting for M3
    VerifyWaitingForM3,
    /// Pairing fully complete
    Complete,
    /// Error state
    Error,
}

/// Result of processing a pairing message
#[derive(Debug)]
pub struct PairingResult {
    /// Response TLV data to send
    pub response: Vec<u8>,
    /// New state
    pub new_state: PairingServerState,
    /// Error (if any)
    pub error: Option<PairingError>,
    /// Pairing complete flag
    pub complete: bool,
}

impl PairingServer {
    /// Create a new pairing server with the given Ed25519 identity
    #[must_use]
    pub fn new(identity: Ed25519KeyPair) -> Self {
        // Generate random salt
        let mut srp_salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut srp_salt);

        Self {
            identity,
            srp_verifier: None,
            srp_salt,
            state: PairingServerState::Idle,
            srp_server: None,
            srp_session_key: None,
            verify_keypair: None,
            shared_secret: None,
            encryption_keys: None,
            client_public_key: None,
            client_curve_public: None,
        }
    }

    /// Set the PIN/password for pairing
    ///
    /// This derives the SRP verifier from the password. For transient
    /// pairing, use a 4-digit PIN. For persistent pairing, use the
    /// configured password.
    pub fn set_password(&mut self, password: &str) {
        let username = b"Pair-Setup";
        let verifier = SrpServer::compute_verifier(
            username,
            password.as_bytes(),
            &self.srp_salt,
            &SrpParams::RFC5054_3072,
        );
        self.srp_verifier = Some(verifier);
    }

    /// Process an incoming pair-setup message
    pub fn process_pair_setup(&mut self, data: &[u8]) -> PairingResult {
        let tlv = match TlvDecoder::decode(data) {
            Ok(t) => t,
            Err(e) => return self.error_result(PairingError::TlvDecode(e.to_string())),
        };

        // Get state from TLV
        let state = tlv.get_state().unwrap_or(0);

        match state {
            1 => self.handle_pair_setup_m1(&tlv),
            3 => self.handle_pair_setup_m3(&tlv),
            _ => self.error_result(PairingError::UnexpectedState(state)),
        }
    }

    /// Process an incoming pair-verify message
    pub fn process_pair_verify(&mut self, data: &[u8]) -> PairingResult {
        let tlv = match TlvDecoder::decode(data) {
            Ok(t) => t,
            Err(e) => return self.error_result(PairingError::TlvDecode(e.to_string())),
        };

        let state = tlv.get_state().unwrap_or(0);

        match state {
            1 => self.handle_pair_verify_m1(&tlv),
            3 => self.handle_pair_verify_m3(&tlv),
            _ => self.error_result(PairingError::UnexpectedState(state)),
        }
    }

    /// Get encryption keys (only valid after successful pairing)
    #[must_use]
    pub fn encryption_keys(&self) -> Option<&EncryptionKeys> {
        self.encryption_keys.as_ref()
    }

    /// Get client's public key (for persistent storage)
    #[must_use]
    pub fn client_public_key(&self) -> Option<&[u8; 32]> {
        self.client_public_key.as_ref()
    }

    /// Reset server state for new pairing attempt
    pub fn reset(&mut self) {
        self.state = PairingServerState::Idle;
        self.srp_server = None;
        self.srp_session_key = None;
        self.verify_keypair = None;
        self.shared_secret = None;
        self.encryption_keys = None;
        self.client_public_key = None;
        self.client_curve_public = None;
        // Regenerate salt for next attempt
        rand::thread_rng().fill_bytes(&mut self.srp_salt);
    }

    // === Internal handlers ===

    fn handle_pair_setup_m1(&mut self, tlv: &TlvDecoder) -> PairingResult {
        if self.state != PairingServerState::Idle {
            // If we receive M1 in other states, we should probably reset and start over
            // but strict implementation might reject. Let's reset for better UX.
            self.reset();
        }

        // Verify method is pair-setup (0)
        let Some(method_bytes) = tlv.get(TlvType::Method) else {
             // If method missing, default to 0? Or error?
             // Assuming default 0 as per previous code logic:
             // let method = ... .unwrap_or(0);
             // But strict check is better.
             // Actually, `tlv.get(TlvType::Method).and_then(|m| m.first()).copied().unwrap_or(0)`
             // If I use let else, I need to handle None.
             return self.error_result(PairingError::UnsupportedMethod(0)); // Should probably be 0
        };
        let method = method_bytes.first().copied().unwrap_or(0);

        if method != 0 {
            return self.error_result(PairingError::UnsupportedMethod(method));
        }

        // Ensure we have a verifier set
        let Some(verifier) = &self.srp_verifier else {
            return self.error_result(PairingError::NoPassword);
        };
        let verifier = verifier.clone();

        // Create SRP server
        let srp_server = SrpServer::new(
            &verifier,
            &SrpParams::RFC5054_3072,
        );

        let server_public = srp_server.public_key();

        // Build M2 response
        let response = TlvEncoder::new()
            .add_state(2)
            .add(TlvType::Salt, &self.srp_salt)
            .add(TlvType::PublicKey, server_public)
            .build();

        self.srp_server = Some(srp_server);
        self.state = PairingServerState::WaitingForM3;

        PairingResult {
            response,
            new_state: self.state,
            error: None,
            complete: false,
        }
    }

    fn handle_pair_setup_m3(&mut self, tlv: &TlvDecoder) -> PairingResult {
        if self.state != PairingServerState::WaitingForM3 {
            return self.error_result(PairingError::InvalidState);
        }

        let Some(srp_server) = self.srp_server.take() else {
            return self.error_result(PairingError::InvalidState);
        };

        // Get client's public key and proof
        let Some(client_public) = tlv.get(TlvType::PublicKey) else {
            return self.error_result(PairingError::MissingField("PublicKey"));
        };

        let Some(client_proof) = tlv.get(TlvType::Proof) else {
            return self.error_result(PairingError::MissingField("Proof"));
        };

        // Compute shared key and verify client's proof
        let Ok((session_key, server_proof)) = srp_server.verify_client(
            b"Pair-Setup",
            &self.srp_salt,
            client_public,
            client_proof,
        ) else {
            return self.error_result(PairingError::AuthenticationFailed);
        };

        // Derive encryption key from session key
        let Ok(enc_key) = derive_key(
            Some(b"Pair-Setup-Encrypt-Salt"),
            session_key.as_bytes(),
            b"Pair-Setup-Encrypt-Info",
            32,
        ) else {
            return self.error_result(PairingError::DecryptionFailed);
        };

        // Encrypt our Ed25519 public key for the client
        let accessory_info = self.build_accessory_info(session_key.as_bytes());
        let encrypted_data = Self::encrypt_accessory_data(&self.identity, &accessory_info, &enc_key);

        // Build M4 response
        let response = TlvEncoder::new()
            .add_state(4)
            .add(TlvType::Proof, &server_proof)
            .add(TlvType::EncryptedData, &encrypted_data)
            .build();

        let mut session_key_bytes = [0u8; 64];
        if session_key.as_bytes().len() <= 64 {
             session_key_bytes[..session_key.as_bytes().len()].copy_from_slice(session_key.as_bytes());
        }

        self.srp_session_key = Some(session_key_bytes);
        self.state = PairingServerState::PairSetupComplete;

        PairingResult {
            response,
            new_state: self.state,
            error: None,
            complete: false,  // Still need pair-verify
        }
    }

    fn handle_pair_verify_m1(&mut self, tlv: &TlvDecoder) -> PairingResult {
        // Pair-verify can happen after pair-setup or for returning clients
        // We allow it from Idle or PairSetupComplete
        if self.state != PairingServerState::PairSetupComplete
            && self.state != PairingServerState::Idle
        {
             self.reset(); // Clear previous session attempts
        }

        // Get client's X25519 public key
        let Some(client_public_bytes) = tlv.get(TlvType::PublicKey) else {
            return self.error_result(PairingError::MissingField("PublicKey"));
        };

        if client_public_bytes.len() != 32 {
             return self.error_result(PairingError::MissingField("PublicKey"));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(client_public_bytes);
        let Ok(client_public) = X25519PublicKey::from_bytes(&arr) else {
             return self.error_result(PairingError::InvalidState);
        };

        self.client_curve_public = Some(arr);

        // Generate our X25519 keypair
        let keypair = X25519KeyPair::generate();
        let shared_secret = keypair.diffie_hellman(&client_public);

        // Derive session key
        let Ok(session_key) = derive_key(
            Some(b"Pair-Verify-Encrypt-Salt"),
            shared_secret.as_bytes(),
            b"Pair-Verify-Encrypt-Info",
            32,
        ) else {
            return self.error_result(PairingError::DecryptionFailed);
        };

        // Build accessory info for signature
        let mut accessory_info = Vec::new();
        accessory_info.extend_from_slice(keypair.public_key().as_bytes());
        accessory_info.extend_from_slice(self.identity.public_key().as_bytes());
        accessory_info.extend_from_slice(client_public.as_bytes());

        // Sign with Ed25519
        let signature = self.identity.sign(&accessory_info);

        // Encrypt signature and identifier
        let sub_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, self.identity.public_key().as_bytes())
            .add(TlvType::Signature, &signature.to_bytes())
            .build();

        let encrypted = Self::encrypt_with_key(&sub_tlv, &session_key, b"PV-Msg02");

        // Build M2 response
        let response = TlvEncoder::new()
            .add_state(2)
            .add(TlvType::PublicKey, keypair.public_key().as_bytes())
            .add(TlvType::EncryptedData, &encrypted)
            .build();

        self.verify_keypair = Some(keypair);
        self.shared_secret = Some(*shared_secret.as_bytes());
        self.state = PairingServerState::VerifyWaitingForM3;

        PairingResult {
            response,
            new_state: self.state,
            error: None,
            complete: false,
        }
    }

    fn handle_pair_verify_m3(&mut self, tlv: &TlvDecoder) -> PairingResult {
        if self.state != PairingServerState::VerifyWaitingForM3 {
            return self.error_result(PairingError::InvalidState);
        }

        let Some(shared_secret) = self.shared_secret else {
            return self.error_result(PairingError::InvalidState);
        };

        // Get encrypted data
        let Some(encrypted_data) = tlv.get(TlvType::EncryptedData) else {
            return self.error_result(PairingError::MissingField("EncryptedData"));
        };

        // Derive decryption key
        let Ok(session_key) = derive_key(
            Some(b"Pair-Verify-Encrypt-Salt"),
            &shared_secret,
            b"Pair-Verify-Encrypt-Info",
            32,
        ) else {
            return self.error_result(PairingError::DecryptionFailed);
        };

        // Decrypt client's signature data
        let Ok(decrypted) = Self::decrypt_with_key(encrypted_data, &session_key, b"PV-Msg03") else {
             return self.error_result(PairingError::DecryptionFailed);
        };

        // Parse sub-TLV
        let Ok(sub_tlv) = TlvDecoder::decode(&decrypted) else {
             return self.error_result(PairingError::TlvDecode("Failed to decode sub-TLV".to_string()));
        };

        // Get client's identifier (Ed25519 public key) and signature
        let Some(client_id) = sub_tlv.get(TlvType::Identifier) else {
            return self.error_result(PairingError::MissingField("Identifier"));
        };

        if client_id.len() != 32 {
             return self.error_result(PairingError::MissingField("Identifier"));
        }
        let mut client_id_arr = [0u8; 32];
        client_id_arr.copy_from_slice(client_id);

        let Some(client_signature) = sub_tlv.get(TlvType::Signature) else {
            return self.error_result(PairingError::MissingField("Signature"));
        };

        // Verify signature
        // Info: ClientCurvePublic || ClientIdentityPublic || ServerCurvePublic
        let Some(client_curve_public) = self.client_curve_public else {
            return self.error_result(PairingError::InvalidState);
        };

        let mut verify_info = Vec::new();
        verify_info.extend_from_slice(&client_curve_public);
        verify_info.extend_from_slice(&client_id_arr);

        let Some(verify_keypair) = &self.verify_keypair else {
             return self.error_result(PairingError::InvalidState);
        };
        verify_info.extend_from_slice(verify_keypair.public_key().as_bytes());

        // Client ID is Ed25519 public key
        // Verify signature using Ed25519
        let Ok(client_identity) = Ed25519PublicKey::from_bytes(&client_id_arr) else {
             return self.error_result(PairingError::AuthenticationFailed);
        };

        let Ok(signature) = Ed25519Signature::from_bytes(client_signature) else {
             return self.error_result(PairingError::SignatureVerificationFailed);
        };

        if client_identity.verify(&verify_info, &signature).is_err() {
             return self.error_result(PairingError::SignatureVerificationFailed);
        }

        // Derive encryption keys for the session
        let enc_keys = Self::derive_session_keys(&shared_secret);

        // Build M4 response (empty encrypted data indicates success)
        let response = TlvEncoder::new()
            .add_state(4)
            .build();

        self.client_public_key = Some(client_id_arr);
        self.encryption_keys = Some(enc_keys);
        self.state = PairingServerState::Complete;

        PairingResult {
            response,
            new_state: self.state,
            error: None,
            complete: true,
        }
    }

    // === Helper methods ===

    fn build_accessory_info(&self, session_key: &[u8]) -> Vec<u8> {
        // Sign: SHA-512(session_key) || identifier || Ed25519 public key
        let mut hasher = Sha512::new();
        hasher.update(session_key);
        let hash = hasher.finalize();

        let mut info = Vec::new();
        info.extend_from_slice(&hash);
        info.extend_from_slice(self.identity.public_key().as_bytes());
        info
    }

    fn encrypt_accessory_data(identity: &Ed25519KeyPair, info: &[u8], key: &[u8]) -> Vec<u8> {
        // Sign the info
        let signature = identity.sign(info);

        // Build sub-TLV with identifier and signature
        let sub_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, identity.public_key().as_bytes())
            .add(TlvType::Signature, &signature.to_bytes())
            .build();

        // Encrypt with ChaCha20-Poly1305
        Self::encrypt_with_key(&sub_tlv, key, b"PS-Msg04")
    }

    fn encrypt_with_key(data: &[u8], key: &[u8], nonce_prefix: &[u8]) -> Vec<u8> {
        let mut nonce_bytes = [0u8; 12];
        let len = nonce_prefix.len().min(12);
        nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
        let nonce = Nonce::from_bytes(&nonce_bytes).expect("nonce creation");

        let cipher = ChaCha20Poly1305Cipher::new(key).expect("cipher creation");
        cipher.encrypt(&nonce, data).expect("encryption failed")
    }

    fn decrypt_with_key(data: &[u8], key: &[u8], nonce_prefix: &[u8]) -> Result<Vec<u8>, PairingError> {
        let mut nonce_bytes = [0u8; 12];
        let len = nonce_prefix.len().min(12);
        nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
        let nonce = Nonce::from_bytes(&nonce_bytes).expect("nonce creation");

        let cipher = ChaCha20Poly1305Cipher::new(key)
            .map_err(|_| PairingError::DecryptionFailed)?;

        cipher.decrypt(&nonce, data)
            .map_err(|_| PairingError::DecryptionFailed)
    }

    fn derive_session_keys(shared_secret: &[u8; 32]) -> EncryptionKeys {
        // Derive keys for bidirectional communication
        let encrypt_key = derive_key(
            Some(b"Control-Salt"),
            shared_secret,
            b"Control-Write-Encryption-Key",
            32,
        ).expect("key derivation");

        let decrypt_key = derive_key(
            Some(b"Control-Salt"),
            shared_secret,
            b"Control-Read-Encryption-Key",
            32,
        ).expect("key derivation");

        EncryptionKeys {
            encrypt_key: encrypt_key.try_into().expect("key length"),
            decrypt_key: decrypt_key.try_into().expect("key length"),
            encrypt_nonce: 0,
            decrypt_nonce: 0,
        }
    }

    fn error_result(&mut self, error: PairingError) -> PairingResult {
        self.state = PairingServerState::Error;

        // Build error TLV
        let error_code = match &error {
            PairingError::AuthenticationFailed => 2,
            PairingError::InvalidState => 6,
            _ => 1,  // Unknown error
        };

        let response = TlvEncoder::new()
            .add_state(0)
            .add_byte(TlvType::Error, error_code)
            .build();

        PairingResult {
            response,
            new_state: self.state,
            error: Some(error),
            complete: false,
        }
    }
}

/// Errors that can occur during pairing
#[derive(Debug, Error)]
pub enum PairingError {
    /// TLV decoding failed
    #[error("TLV decode error: {0}")]
    TlvDecode(String),

    /// Unexpected pairing state/sequence
    #[error("Unexpected pairing state: {0}")]
    UnexpectedState(u8),

    /// Internal state machine error
    #[error("Invalid state machine state")]
    InvalidState,

    /// Unsupported pairing method
    #[error("Unsupported pairing method: {0}")]
    UnsupportedMethod(u8),

    /// Password/PIN not configured
    #[error("No password/PIN configured")]
    NoPassword,

    /// Required TLV field missing
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    /// Authentication failed (wrong PIN)
    #[error("Authentication failed - wrong PIN/password")]
    AuthenticationFailed,

    /// Decryption failed
    #[error("Decryption failed")]
    DecryptionFailed,

    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_server() -> PairingServer {
        let identity = Ed25519KeyPair::generate();
        let mut server = PairingServer::new(identity);
        server.set_password("1234");
        server
    }

    #[test]
    fn test_initial_state() {
        let server = create_test_server();
        assert_eq!(server.state, PairingServerState::Idle);
    }

    #[test]
    fn test_m1_handling() {
        let mut server = create_test_server();

        // Build M1 TLV
        let m1 = TlvEncoder::new()
            .add_state(1)
            .add_byte(TlvType::Method, 0)
            .build();

        let result = server.process_pair_setup(&m1);

        assert!(result.error.is_none());
        assert_eq!(result.new_state, PairingServerState::WaitingForM3);

        // Response should contain state=2, salt, and public key
        let response_tlv = TlvDecoder::decode(&result.response).unwrap();
        assert_eq!(response_tlv.get_state().ok(), Some(2));
        assert!(response_tlv.get(TlvType::Salt).is_some());
        assert!(response_tlv.get(TlvType::PublicKey).is_some());
    }

    #[test]
    fn test_state_machine_enforcement() {
        let mut server = create_test_server();

        // Try M3 before M1 - should fail
        let m3 = TlvEncoder::new()
            .add_state(3)
            .build();

        let result = server.process_pair_setup(&m3);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_reset() {
        let mut server = create_test_server();

        // Process M1
        let m1 = TlvEncoder::new()
            .add_state(1)
            .add_byte(TlvType::Method, 0)
            .build();

        let _ = server.process_pair_setup(&m1);
        assert_eq!(server.state, PairingServerState::WaitingForM3);

        // Reset
        server.reset();
        assert_eq!(server.state, PairingServerState::Idle);
    }
}
