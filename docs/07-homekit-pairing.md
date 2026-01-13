# Section 07: HomeKit Pairing Protocol

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 03**: Binary Plist Codec (must be complete)
- **Section 04**: Cryptographic Primitives (must be complete)

## Overview

AirPlay 2 devices require HomeKit-style pairing for authentication. This section implements the pairing protocols:

1. **Transient Pairing**: Quick pairing without persistent keys (used for most connections)
2. **Pair-Setup**: Initial PIN-based pairing to establish long-term keys
3. **Pair-Verify**: Fast verification using previously established keys

The pairing protocol uses:
- SRP-6a for PIN verification
- Ed25519 for signatures
- X25519 for key exchange
- HKDF for key derivation
- ChaCha20-Poly1305 for encrypted messages

## Objectives

- Implement transient pairing (most common case)
- Implement Pair-Setup for PIN-based pairing
- Implement Pair-Verify for persistent pairing
- Handle encrypted session establishment
- Support pairing key storage (optional)

---

## Tasks

### 7.1 Pairing Types and State

- [ ] **7.1.1** Define pairing state machine

**File:** `src/protocol/pairing/mod.rs`

```rust
//! HomeKit pairing protocol implementation

mod transient;
mod setup;
mod verify;
mod storage;
mod tlv;

pub use transient::TransientPairing;
pub use setup::PairSetup;
pub use verify::PairVerify;
pub use storage::{PairingStorage, PairingKeys};
pub use tlv::{TlvType, TlvEncoder, TlvDecoder, TlvError};

use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};

/// Pairing session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    /// Initial state
    Init,
    /// Waiting for device response
    WaitingResponse,
    /// SRP exchange in progress (Pair-Setup)
    SrpExchange,
    /// Key exchange in progress
    KeyExchange,
    /// Verifying signatures
    Verifying,
    /// Pairing complete
    Complete,
    /// Pairing failed
    Failed,
}

/// Result of a pairing step
#[derive(Debug)]
pub enum PairingStepResult {
    /// Need to send data to device
    SendData(Vec<u8>),
    /// Need more data from device
    NeedData,
    /// Pairing complete, here are the session keys
    Complete(SessionKeys),
    /// Pairing failed
    Failed(PairingError),
}

/// Established session keys after pairing
pub struct SessionKeys {
    /// Key for encrypting data sent to device
    pub encrypt_key: [u8; 32],
    /// Key for decrypting data from device
    pub decrypt_key: [u8; 32],
    /// Initial nonce for encryption
    pub encrypt_nonce: u64,
    /// Initial nonce for decryption
    pub decrypt_nonce: u64,
}

impl SessionKeys {
    /// Create cipher for encrypting outgoing messages
    pub fn encryptor(&self) -> Result<EncryptedChannel, crate::protocol::crypto::CryptoError> {
        EncryptedChannel::new(&self.encrypt_key, self.encrypt_nonce, true)
    }

    /// Create cipher for decrypting incoming messages
    pub fn decryptor(&self) -> Result<EncryptedChannel, crate::protocol::crypto::CryptoError> {
        EncryptedChannel::new(&self.decrypt_key, self.decrypt_nonce, false)
    }
}

/// Encrypted channel for post-pairing communication
pub struct EncryptedChannel {
    cipher: ChaCha20Poly1305Cipher,
    nonce_counter: u64,
    is_sender: bool,
}

impl EncryptedChannel {
    /// Create a new encrypted channel
    pub fn new(key: &[u8], initial_nonce: u64, is_sender: bool) -> Result<Self, crate::protocol::crypto::CryptoError> {
        let cipher = ChaCha20Poly1305Cipher::new(key)?;
        Ok(Self {
            cipher,
            nonce_counter: initial_nonce,
            is_sender,
        })
    }

    /// Encrypt a message
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let nonce = Nonce::from_counter(self.nonce_counter);
        self.nonce_counter += 1;
        self.cipher.encrypt(&nonce, plaintext)
    }

    /// Decrypt a message
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let nonce = Nonce::from_counter(self.nonce_counter);
        self.nonce_counter += 1;
        self.cipher.decrypt(&nonce, ciphertext)
    }

    /// Encrypt with length prefix (for framed protocols)
    pub fn encrypt_framed(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, crate::protocol::crypto::CryptoError> {
        let encrypted = self.encrypt(plaintext)?;
        let mut output = Vec::with_capacity(2 + encrypted.len());
        output.extend_from_slice(&(encrypted.len() as u16).to_le_bytes());
        output.extend_from_slice(&encrypted);
        Ok(output)
    }
}

/// Pairing errors
#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("invalid state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("invalid TLV: {0}")]
    InvalidTlv(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("SRP verification failed")]
    SrpVerificationFailed,

    #[error("signature verification failed")]
    SignatureVerificationFailed,

    #[error("crypto error: {0}")]
    CryptoError(#[from] crate::protocol::crypto::CryptoError),

    #[error("device returned error: {code}")]
    DeviceError { code: u8 },

    #[error("pairing not supported by device")]
    NotSupported,

    #[error("pairing required (no stored keys)")]
    PairingRequired,

    #[error("stored keys invalid")]
    InvalidStoredKeys,
}
```

---

### 7.2 TLV Encoding

- [ ] **7.2.1** Implement TLV (Type-Length-Value) codec

**File:** `src/protocol/pairing/tlv.rs`

```rust
//! TLV8 encoding for HomeKit pairing protocol

use std::collections::HashMap;
use thiserror::Error;

/// TLV type codes used in HomeKit pairing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TlvType {
    /// Method to use (pairing method)
    Method = 0x00,
    /// Pairing identifier
    Identifier = 0x01,
    /// Salt for SRP
    Salt = 0x02,
    /// Public key
    PublicKey = 0x03,
    /// Proof (M1/M2 in SRP)
    Proof = 0x04,
    /// Encrypted data
    EncryptedData = 0x05,
    /// Pairing state/sequence number
    State = 0x06,
    /// Error code
    Error = 0x07,
    /// Retry delay
    RetryDelay = 0x08,
    /// Certificate
    Certificate = 0x09,
    /// Signature
    Signature = 0x0A,
    /// Permissions
    Permissions = 0x0B,
    /// Fragment data
    FragmentData = 0x0C,
    /// Fragment last
    FragmentLast = 0x0D,
    /// Session ID
    SessionID = 0x0E,
    /// Flags
    Flags = 0x13,
    /// Separator (empty value, used to separate items)
    Separator = 0xFF,
}

impl TlvType {
    /// Create from byte value
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Method),
            0x01 => Some(Self::Identifier),
            0x02 => Some(Self::Salt),
            0x03 => Some(Self::PublicKey),
            0x04 => Some(Self::Proof),
            0x05 => Some(Self::EncryptedData),
            0x06 => Some(Self::State),
            0x07 => Some(Self::Error),
            0x08 => Some(Self::RetryDelay),
            0x09 => Some(Self::Certificate),
            0x0A => Some(Self::Signature),
            0x0B => Some(Self::Permissions),
            0x0C => Some(Self::FragmentData),
            0x0D => Some(Self::FragmentLast),
            0x0E => Some(Self::SessionID),
            0x13 => Some(Self::Flags),
            0xFF => Some(Self::Separator),
            _ => None,
        }
    }
}

/// TLV encoding errors
#[derive(Debug, Error)]
pub enum TlvError {
    #[error("buffer too small")]
    BufferTooSmall,

    #[error("invalid TLV structure")]
    InvalidStructure,

    #[error("unknown type: 0x{0:02x}")]
    UnknownType(u8),

    #[error("missing required field: {0:?}")]
    MissingField(TlvType),

    #[error("invalid value for {0:?}")]
    InvalidValue(TlvType),
}

/// TLV encoder
pub struct TlvEncoder {
    buffer: Vec<u8>,
}

impl TlvEncoder {
    /// Create a new encoder
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Add a TLV item
    pub fn add(&mut self, tlv_type: TlvType, value: &[u8]) -> &mut Self {
        // TLV8 limits each chunk to 255 bytes
        // For larger values, we need to fragment across multiple TLVs
        for chunk in value.chunks(255) {
            self.buffer.push(tlv_type as u8);
            self.buffer.push(chunk.len() as u8);
            self.buffer.extend_from_slice(chunk);
        }

        // Handle empty value
        if value.is_empty() {
            self.buffer.push(tlv_type as u8);
            self.buffer.push(0);
        }

        self
    }

    /// Add a single byte value
    pub fn add_byte(&mut self, tlv_type: TlvType, value: u8) -> &mut Self {
        self.add(tlv_type, &[value])
    }

    /// Add state value
    pub fn add_state(&mut self, state: u8) -> &mut Self {
        self.add_byte(TlvType::State, state)
    }

    /// Add method value
    pub fn add_method(&mut self, method: u8) -> &mut Self {
        self.add_byte(TlvType::Method, method)
    }

    /// Build the encoded TLV data
    pub fn build(self) -> Vec<u8> {
        self.buffer
    }
}

impl Default for TlvEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// TLV decoder
pub struct TlvDecoder {
    items: HashMap<u8, Vec<u8>>,
}

impl TlvDecoder {
    /// Decode TLV data
    pub fn decode(data: &[u8]) -> Result<Self, TlvError> {
        let mut items: HashMap<u8, Vec<u8>> = HashMap::new();
        let mut pos = 0;

        while pos < data.len() {
            if pos + 2 > data.len() {
                return Err(TlvError::BufferTooSmall);
            }

            let tlv_type = data[pos];
            let length = data[pos + 1] as usize;
            pos += 2;

            if pos + length > data.len() {
                return Err(TlvError::BufferTooSmall);
            }

            let value = &data[pos..pos + length];
            pos += length;

            // Concatenate fragmented values
            items
                .entry(tlv_type)
                .or_insert_with(Vec::new)
                .extend_from_slice(value);
        }

        Ok(Self { items })
    }

    /// Get a value by type
    pub fn get(&self, tlv_type: TlvType) -> Option<&[u8]> {
        self.items.get(&(tlv_type as u8)).map(|v| v.as_slice())
    }

    /// Get a required value
    pub fn get_required(&self, tlv_type: TlvType) -> Result<&[u8], TlvError> {
        self.get(tlv_type).ok_or(TlvError::MissingField(tlv_type))
    }

    /// Get state value
    pub fn get_state(&self) -> Result<u8, TlvError> {
        let value = self.get_required(TlvType::State)?;
        if value.len() != 1 {
            return Err(TlvError::InvalidValue(TlvType::State));
        }
        Ok(value[0])
    }

    /// Get error value (if present)
    pub fn get_error(&self) -> Option<u8> {
        self.get(TlvType::Error).and_then(|v| v.first().copied())
    }

    /// Check if an error is present
    pub fn has_error(&self) -> bool {
        self.get(TlvType::Error).is_some()
    }
}

/// Pairing method constants
pub mod methods {
    /// Pair-Setup
    pub const PAIR_SETUP: u8 = 0;
    /// Pair-Setup with auth (MFi)
    pub const PAIR_SETUP_AUTH: u8 = 1;
    /// Pair-Verify
    pub const PAIR_VERIFY: u8 = 2;
    /// Add pairing
    pub const ADD_PAIRING: u8 = 3;
    /// Remove pairing
    pub const REMOVE_PAIRING: u8 = 4;
    /// List pairings
    pub const LIST_PAIRINGS: u8 = 5;
}

/// Error codes from device
pub mod errors {
    pub const UNKNOWN: u8 = 0x01;
    pub const AUTHENTICATION: u8 = 0x02;
    pub const BACKOFF: u8 = 0x03;
    pub const MAX_PEERS: u8 = 0x04;
    pub const MAX_TRIES: u8 = 0x05;
    pub const UNAVAILABLE: u8 = 0x06;
    pub const BUSY: u8 = 0x07;
}
```

---

### 7.3 Transient Pairing

- [ ] **7.3.1** Implement transient pairing (no PIN required)

**File:** `src/protocol/pairing/transient.rs`

```rust
//! Transient pairing - quick pairing without stored keys
//!
//! This is the simplest pairing method, used when:
//! - Device allows unauthenticated connections
//! - We don't need to store keys for later

use super::{PairingState, PairingStepResult, PairingError, SessionKeys, tlv::*};
use crate::protocol::crypto::{
    X25519KeyPair, X25519PublicKey, Ed25519KeyPair,
    HkdfSha512, ChaCha20Poly1305Cipher, Nonce,
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
    pub fn new() -> Result<Self, PairingError> {
        let our_keypair = X25519KeyPair::generate();
        let signing_keypair = Ed25519KeyPair::generate()?;

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
    pub fn state(&self) -> PairingState {
        self.state
    }

    /// Start pairing - returns M1 message
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
        let shared_secret = self.shared_secret.as_ref()
            .ok_or(PairingError::InvalidState {
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
            PairingState::Complete => {
                Ok(PairingStepResult::Complete(
                    self.session_keys.clone().unwrap()
                ))
            }
            PairingState::Failed => {
                Err(PairingError::InvalidState {
                    expected: "not failed".to_string(),
                    actual: "Failed".to_string(),
                })
            }
            _ => Ok(PairingStepResult::NeedData),
        }
    }
}

impl Default for TransientPairing {
    fn default() -> Self {
        Self::new().expect("failed to create transient pairing")
    }
}
```

---

### 7.4 Pair-Setup (PIN-based)

- [ ] **7.4.1** Implement Pair-Setup with SRP

**File:** `src/protocol/pairing/setup.rs`

```rust
//! Pair-Setup - PIN-based pairing using SRP-6a
//!
//! This is used when first connecting to a device that requires authentication.
//! The user must enter a PIN displayed on the device.

use super::{PairingState, PairingStepResult, PairingError, SessionKeys, tlv::*};
use crate::protocol::crypto::{
    SrpClient, Ed25519KeyPair, X25519KeyPair,
    HkdfSha512, ChaCha20Poly1305Cipher, Nonce,
};

/// Pair-Setup session for PIN-based pairing
pub struct PairSetup {
    state: PairingState,
    /// PIN entered by user
    pin: Option<String>,
    /// SRP client
    srp_client: Option<SrpClient>,
    /// Our Ed25519 long-term key pair
    signing_keypair: Ed25519KeyPair,
    /// Session key from SRP
    session_key: Option<Vec<u8>>,
    /// Device's Ed25519 public key (for verification)
    device_ltpk: Option<Vec<u8>>,
}

impl PairSetup {
    /// Create a new Pair-Setup session
    pub fn new() -> Result<Self, PairingError> {
        let signing_keypair = Ed25519KeyPair::generate()?;

        Ok(Self {
            state: PairingState::Init,
            pin: None,
            srp_client: None,
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
        let pin = self.pin.as_ref()
            .ok_or(PairingError::AuthenticationFailed("PIN not set".to_string()))?;

        // Create SRP client and process challenge
        let srp_client = SrpClient::new()?;
        let client_public = srp_client.public_key().to_vec();

        let verifier = srp_client.process_challenge(
            b"Pair-Setup",
            pin.as_bytes(),
            salt,
            server_public,
        )?;

        // Build M3: state=3, public_key=A, proof=M1
        let m3 = TlvEncoder::new()
            .add_state(3)
            .add(TlvType::PublicKey, &client_public)
            .add(TlvType::Proof, verifier.client_proof())
            .build();

        self.srp_client = Some(srp_client);
        self.state = PairingState::SrpExchange;

        Ok(PairingStepResult::SendData(m3))
    }

    /// Process M4 (server proof) and generate M5
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

        // TODO: Verify server proof with SRP verifier
        // For now, assume it's valid and derive session key

        // Derive session key using HKDF
        // In real implementation, use the SRP session key
        let session_key = vec![0u8; 32]; // Placeholder

        // Derive encryption key for M5
        let hkdf = HkdfSha512::new(
            Some(b"Pair-Setup-Encrypt-Salt"),
            &session_key,
        );
        let encrypt_key = hkdf.expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")?;

        // Build inner TLV with our identifier and public key
        let inner_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, b"airplay2-rs")
            .add(TlvType::PublicKey, self.signing_keypair.public_key().as_bytes())
            .build();

        // Sign: HKDF(...) || identifier || public_key
        let mut sign_data = hkdf.expand(b"Pair-Setup-Controller-Sign-Salt", 32)?;
        sign_data.extend_from_slice(b"airplay2-rs");
        sign_data.extend_from_slice(self.signing_keypair.public_key().as_bytes());

        let signature = self.signing_keypair.sign(&sign_data);

        let signed_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, b"airplay2-rs")
            .add(TlvType::PublicKey, self.signing_keypair.public_key().as_bytes())
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

        let session_key = self.session_key.as_ref()
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
    pub fn our_public_key(&self) -> &[u8; 32] {
        self.signing_keypair.public_key().as_bytes()
    }

    /// Get our long-term secret key (for storage)
    pub fn our_secret_key(&self) -> [u8; 32] {
        self.signing_keypair.secret_bytes()
    }

    /// Get device's long-term public key (for storage)
    pub fn device_public_key(&self) -> Option<&[u8]> {
        self.device_ltpk.as_deref()
    }
}
```

---

### 7.5 Pair-Verify

- [ ] **7.5.1** Implement Pair-Verify for stored keys

**File:** `src/protocol/pairing/verify.rs`

```rust
//! Pair-Verify - Fast verification using stored keys
//!
//! Used after initial Pair-Setup to quickly establish a session
//! without requiring PIN entry again.

use super::{PairingState, PairingStepResult, PairingError, SessionKeys, PairingKeys, tlv::*};
use crate::protocol::crypto::{
    Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature,
    X25519KeyPair, X25519PublicKey,
    HkdfSha512, ChaCha20Poly1305Cipher, Nonce,
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
}

impl PairVerify {
    /// Create a new Pair-Verify session with stored keys
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
            .add(TlvType::PublicKey, self.ephemeral_keypair.public_key().as_bytes())
            .build();

        self.state = PairingState::WaitingResponse;
        Ok(m1)
    }

    /// Process M2 and generate M3
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
        let hkdf = HkdfSha512::new(
            Some(b"Pair-Verify-Encrypt-Salt"),
            shared.as_bytes(),
        );
        let session_key = hkdf.expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")?;

        // Decrypt device's signature
        let cipher = ChaCha20Poly1305Cipher::new(&session_key)?;
        let nonce = Nonce::from_bytes(&[0u8; 12])?;
        let decrypted = cipher.decrypt(&nonce, encrypted_data)?;

        let device_tlv = TlvDecoder::decode(&decrypted)?;
        let device_identifier = device_tlv.get_required(TlvType::Identifier)?;
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

        let nonce = Nonce::from_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])?;
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
        let shared_secret = self.shared_secret.as_ref()
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
```

---

### 7.6 Pairing Storage

- [ ] **7.6.1** Implement pairing key storage

**File:** `src/protocol/pairing/storage.rs`

```rust
//! Storage for pairing keys

use std::path::Path;
use std::collections::HashMap;

/// Stored pairing keys for a device
#[derive(Debug, Clone)]
pub struct PairingKeys {
    /// Our identifier (e.g., "airplay2-rs")
    pub identifier: Vec<u8>,
    /// Our Ed25519 secret key (32 bytes)
    pub secret_key: [u8; 32],
    /// Our Ed25519 public key (32 bytes)
    pub public_key: [u8; 32],
    /// Device's Ed25519 public key (32 bytes)
    pub device_public_key: [u8; 32],
}

/// Abstract storage interface for pairing keys
pub trait PairingStorage: Send + Sync {
    /// Load keys for a device
    fn load(&self, device_id: &str) -> Option<PairingKeys>;

    /// Save keys for a device
    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError>;

    /// Remove keys for a device
    fn remove(&mut self, device_id: &str) -> Result<(), StorageError>;

    /// List all stored device IDs
    fn list_devices(&self) -> Vec<String>;
}

/// Storage errors
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("storage not available")]
    NotAvailable,
}

/// In-memory pairing storage (non-persistent)
#[derive(Debug, Default)]
pub struct MemoryStorage {
    keys: HashMap<String, PairingKeys>,
}

impl MemoryStorage {
    /// Create a new in-memory storage
    pub fn new() -> Self {
        Self::default()
    }
}

impl PairingStorage for MemoryStorage {
    fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.keys.get(device_id).cloned()
    }

    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.keys.insert(device_id.to_string(), keys.clone());
        Ok(())
    }

    fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.keys.remove(device_id);
        Ok(())
    }

    fn list_devices(&self) -> Vec<String> {
        self.keys.keys().cloned().collect()
    }
}

/// File-based pairing storage
#[cfg(feature = "persistent-pairing")]
pub struct FileStorage {
    path: std::path::PathBuf,
    cache: HashMap<String, PairingKeys>,
}

#[cfg(feature = "persistent-pairing")]
impl FileStorage {
    /// Create file storage at the given path
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Load existing keys
        let cache = Self::load_all(&path)?;

        Ok(Self { path, cache })
    }

    fn load_all(path: &Path) -> Result<HashMap<String, PairingKeys>, StorageError> {
        // Implementation would read from file/database
        Ok(HashMap::new())
    }

    fn save_all(&self) -> Result<(), StorageError> {
        // Implementation would write to file/database
        Ok(())
    }
}

#[cfg(feature = "persistent-pairing")]
impl PairingStorage for FileStorage {
    fn load(&self, device_id: &str) -> Option<PairingKeys> {
        self.cache.get(device_id).cloned()
    }

    fn save(&mut self, device_id: &str, keys: &PairingKeys) -> Result<(), StorageError> {
        self.cache.insert(device_id.to_string(), keys.clone());
        self.save_all()
    }

    fn remove(&mut self, device_id: &str) -> Result<(), StorageError> {
        self.cache.remove(device_id);
        self.save_all()
    }

    fn list_devices(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/pairing/tlv.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlv_encode_simple() {
        let encoded = TlvEncoder::new()
            .add_state(1)
            .add_method(0)
            .build();

        assert_eq!(encoded, vec![
            0x06, 0x01, 0x01,  // State = 1
            0x00, 0x01, 0x00,  // Method = 0
        ]);
    }

    #[test]
    fn test_tlv_decode_simple() {
        let data = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
        let decoder = TlvDecoder::decode(&data).unwrap();

        assert_eq!(decoder.get_state().unwrap(), 1);
        assert_eq!(decoder.get(TlvType::Method), Some(&[0u8][..]));
    }

    #[test]
    fn test_tlv_fragmentation() {
        // Data longer than 255 bytes should be fragmented
        let long_data = vec![0xAA; 300];
        let encoded = TlvEncoder::new()
            .add(TlvType::PublicKey, &long_data)
            .build();

        // Should have two TLV entries
        assert_eq!(encoded[0], TlvType::PublicKey as u8);
        assert_eq!(encoded[1], 255); // First chunk is max size

        // Decode should reassemble
        let decoder = TlvDecoder::decode(&encoded).unwrap();
        let decoded = decoder.get(TlvType::PublicKey).unwrap();
        assert_eq!(decoded, &long_data[..]);
    }

    #[test]
    fn test_tlv_error_detection() {
        let data = vec![0x07, 0x01, 0x02]; // Error = 2
        let decoder = TlvDecoder::decode(&data).unwrap();

        assert!(decoder.has_error());
        assert_eq!(decoder.get_error(), Some(2));
    }

    #[test]
    fn test_tlv_missing_field() {
        let data = vec![0x06, 0x01, 0x01]; // Only state
        let decoder = TlvDecoder::decode(&data).unwrap();

        let result = decoder.get_required(TlvType::PublicKey);
        assert!(matches!(result, Err(TlvError::MissingField(_))));
    }
}
```

### Test File: `src/protocol/pairing/transient.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_start() {
        let mut pairing = TransientPairing::new().unwrap();
        let m1 = pairing.start().unwrap();

        let decoder = TlvDecoder::decode(&m1).unwrap();
        assert_eq!(decoder.get_state().unwrap(), 1);
        assert!(decoder.get(TlvType::PublicKey).is_some());
    }

    #[test]
    fn test_transient_invalid_state() {
        let mut pairing = TransientPairing::new().unwrap();

        // Try to process M2 without starting
        let result = pairing.process_m2(&[]);
        assert!(matches!(result, Err(PairingError::InvalidState { .. })));
    }

    #[test]
    fn test_transient_device_error() {
        let mut pairing = TransientPairing::new().unwrap();
        pairing.start().unwrap();

        // Simulate device error response
        let m2 = TlvEncoder::new()
            .add_state(2)
            .add_byte(TlvType::Error, errors::AUTHENTICATION)
            .build();

        let result = pairing.process_m2(&m2);
        assert!(matches!(result, Err(PairingError::DeviceError { code: 2 })));
    }
}
```

---

## Acceptance Criteria

- [ ] TLV encoding/decoding handles fragmentation correctly
- [ ] Transient pairing produces valid M1/M3 messages
- [ ] Pair-Setup integrates with SRP correctly
- [ ] Pair-Verify validates device signatures
- [ ] Session keys are derived correctly
- [ ] Encrypted channel encrypts/decrypts messages
- [ ] Storage interface is implemented
- [ ] All unit tests pass
- [ ] Error handling covers all failure modes

---

## Notes

- The exact HKDF salt/info strings may need adjustment based on protocol analysis
- SRP integration requires careful handling of big integers
- Device signature verification is critical for security
- Consider adding rate limiting for failed pairing attempts
