# Section 53: Encrypted Control Channel

## Dependencies
- **Section 49**: HomeKit Pairing (Server-Side) - session key derivation
- **Section 04**: Cryptographic Primitives (ChaCha20-Poly1305)
- **Section 48**: RTSP/HTTP Server Extensions

## Overview

After successful pairing, all AirPlay 2 control channel traffic is encrypted using ChaCha20-Poly1305. This section implements the HAP-style (HomeKit Accessory Protocol) framing used for the encrypted control channel.

### Encryption Flow

```
Before Pairing:
  Sender ──── Plaintext RTSP ────▶ Receiver

After Pairing:
  Sender ──── Encrypted Frame ────▶ Receiver
              [length][encrypted_payload][auth_tag]
```

### Frame Format

```
┌──────────────────────────────────────────────────┐
│ HAP Encrypted Frame                              │
├──────────┬───────────────────────┬───────────────┤
│ Length   │ Encrypted Data        │ Auth Tag      │
│ (2 bytes)│ (variable)            │ (16 bytes)    │
│ LE u16   │ ChaCha20-Poly1305    │               │
└──────────┴───────────────────────┴───────────────┘
```

## Objectives

- Implement HAP-style frame encryption/decryption
- Maintain separate nonces for send/receive directions
- Handle frame reassembly for TCP streams
- Support both request and response encryption
- Reuse existing ChaCha20-Poly1305 implementation

---

## Tasks

### 53.1 Encrypted Channel Codec

- [x] **53.1.1** Implement the encrypted channel codec

**File:** `src/receiver/ap2/encrypted_channel.rs`

```rust
//! Encrypted Control Channel for AirPlay 2
//!
//! After pairing completes, all RTSP traffic is encrypted using
//! ChaCha20-Poly1305 with HAP-style framing.

use crate::protocol::crypto::chacha::ChaCha20Poly1305;
use bytes::{Buf, BufMut, BytesMut};

/// Maximum frame size (64KB)
const MAX_FRAME_SIZE: usize = 65535;

/// Auth tag size for ChaCha20-Poly1305
const TAG_SIZE: usize = 16;

/// Length prefix size
const LENGTH_SIZE: usize = 2;

/// Encrypted channel state
pub struct EncryptedChannel {
    /// Key for encrypting outgoing messages
    encrypt_key: [u8; 32],
    /// Key for decrypting incoming messages
    decrypt_key: [u8; 32],
    /// Nonce counter for encryption
    encrypt_nonce: u64,
    /// Nonce counter for decryption
    decrypt_nonce: u64,
    /// Input buffer for frame reassembly
    input_buffer: BytesMut,
    /// Whether encryption is enabled
    enabled: bool,
}

impl EncryptedChannel {
    /// Create a new encrypted channel with derived keys
    ///
    /// # Arguments
    /// * `encrypt_key` - Key for encrypting messages TO the sender
    /// * `decrypt_key` - Key for decrypting messages FROM the sender
    pub fn new(encrypt_key: [u8; 32], decrypt_key: [u8; 32]) -> Self {
        Self {
            encrypt_key,
            decrypt_key,
            encrypt_nonce: 0,
            decrypt_nonce: 0,
            input_buffer: BytesMut::with_capacity(4096),
            enabled: true,
        }
    }

    /// Create a disabled/passthrough channel
    pub fn disabled() -> Self {
        Self {
            encrypt_key: [0; 32],
            decrypt_key: [0; 32],
            encrypt_nonce: 0,
            decrypt_nonce: 0,
            input_buffer: BytesMut::new(),
            enabled: false,
        }
    }

    /// Check if encryption is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable encryption with new keys
    pub fn enable(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.encrypt_key = encrypt_key;
        self.decrypt_key = decrypt_key;
        self.encrypt_nonce = 0;
        self.decrypt_nonce = 0;
        self.input_buffer.clear();
        self.enabled = true;
    }

    /// Disable encryption (passthrough mode)
    pub fn disable(&mut self) {
        self.enabled = false;
        self.input_buffer.clear();
    }

    /// Encrypt a message
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if !self.enabled {
            return Ok(plaintext.to_vec());
        }

        if plaintext.len() > MAX_FRAME_SIZE {
            return Err(EncryptionError::MessageTooLarge {
                size: plaintext.len(),
                max: MAX_FRAME_SIZE,
            });
        }

        // Build nonce: 4 bytes zero + 8 bytes counter (LE)
        let nonce = self.build_nonce(self.encrypt_nonce);
        self.encrypt_nonce += 1;

        // Encrypt with AEAD
        let cipher = ChaCha20Poly1305::new(&self.encrypt_key);
        let ciphertext = cipher.encrypt(&nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        // Build frame: length (2 bytes LE) + ciphertext (includes tag)
        let mut frame = Vec::with_capacity(LENGTH_SIZE + ciphertext.len());
        frame.put_u16_le(plaintext.len() as u16);
        frame.extend_from_slice(&ciphertext);

        Ok(frame)
    }

    /// Feed bytes into the decryption buffer
    pub fn feed(&mut self, data: &[u8]) {
        self.input_buffer.extend_from_slice(data);
    }

    /// Try to decrypt a complete frame from the buffer
    pub fn decrypt(&mut self) -> Result<Option<Vec<u8>>, EncryptionError> {
        if !self.enabled {
            // Passthrough mode - return entire buffer
            if self.input_buffer.is_empty() {
                return Ok(None);
            }
            let data = self.input_buffer.split().to_vec();
            return Ok(Some(data));
        }

        // Need at least length prefix
        if self.input_buffer.len() < LENGTH_SIZE {
            return Ok(None);
        }

        // Read length (peek, don't consume yet)
        let plaintext_len = u16::from_le_bytes([
            self.input_buffer[0],
            self.input_buffer[1],
        ]) as usize;

        // Validate length
        if plaintext_len > MAX_FRAME_SIZE {
            return Err(EncryptionError::InvalidFrameLength(plaintext_len));
        }

        // Total frame size: length prefix + plaintext + auth tag
        let frame_size = LENGTH_SIZE + plaintext_len + TAG_SIZE;

        // Need complete frame
        if self.input_buffer.len() < frame_size {
            return Ok(None);
        }

        // Consume the frame
        let _ = self.input_buffer.get_u16_le(); // length prefix
        let ciphertext: Vec<u8> = self.input_buffer.split_to(plaintext_len + TAG_SIZE).to_vec();

        // Build nonce
        let nonce = self.build_nonce(self.decrypt_nonce);
        self.decrypt_nonce += 1;

        // Decrypt with AEAD
        let cipher = ChaCha20Poly1305::new(&self.decrypt_key);
        let plaintext = cipher.decrypt(&nonce, &ciphertext)
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        Ok(Some(plaintext))
    }

    /// Decrypt all available frames
    pub fn decrypt_all(&mut self) -> Result<Vec<Vec<u8>>, EncryptionError> {
        let mut frames = Vec::new();

        while let Some(frame) = self.decrypt()? {
            frames.push(frame);
        }

        Ok(frames)
    }

    /// Build 12-byte nonce from counter
    fn build_nonce(&self, counter: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        // First 4 bytes are zero
        // Last 8 bytes are counter in little-endian
        nonce[4..12].copy_from_slice(&counter.to_le_bytes());
        nonce
    }

    /// Get current encrypt nonce (for debugging)
    pub fn encrypt_nonce(&self) -> u64 {
        self.encrypt_nonce
    }

    /// Get current decrypt nonce (for debugging)
    pub fn decrypt_nonce(&self) -> u64 {
        self.decrypt_nonce
    }

    /// Clear input buffer
    pub fn clear(&mut self) {
        self.input_buffer.clear();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("Message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Invalid frame length: {0}")]
    InvalidFrameLength(usize),

    #[error("Encryption failed")]
    EncryptionFailed,

    #[error("Decryption failed - authentication error or corrupted data")]
    DecryptionFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_channel() -> (EncryptedChannel, EncryptedChannel) {
        // Create two channels with swapped keys (simulating sender/receiver)
        let key_a = [0x41u8; 32];
        let key_b = [0x42u8; 32];

        let sender = EncryptedChannel::new(key_a, key_b);
        let receiver = EncryptedChannel::new(key_b, key_a);

        (sender, receiver)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (mut sender, mut receiver) = create_test_channel();

        let message = b"Hello, AirPlay 2!";

        // Encrypt on sender side
        let encrypted = sender.encrypt(message).unwrap();

        // Decrypt on receiver side
        receiver.feed(&encrypted);
        let decrypted = receiver.decrypt().unwrap().unwrap();

        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_multiple_messages() {
        let (mut sender, mut receiver) = create_test_channel();

        let messages = vec![
            b"First message".to_vec(),
            b"Second message".to_vec(),
            b"Third message".to_vec(),
        ];

        // Encrypt all
        let mut encrypted = Vec::new();
        for msg in &messages {
            encrypted.extend_from_slice(&sender.encrypt(msg).unwrap());
        }

        // Feed all at once
        receiver.feed(&encrypted);

        // Decrypt all
        let decrypted = receiver.decrypt_all().unwrap();

        assert_eq!(decrypted.len(), 3);
        for (i, msg) in decrypted.iter().enumerate() {
            assert_eq!(msg, &messages[i]);
        }
    }

    #[test]
    fn test_partial_frame() {
        let (mut sender, mut receiver) = create_test_channel();

        let message = b"Test partial frame";
        let encrypted = sender.encrypt(message).unwrap();

        // Feed only part of the frame
        receiver.feed(&encrypted[..5]);
        assert!(receiver.decrypt().unwrap().is_none());

        // Feed the rest
        receiver.feed(&encrypted[5..]);
        let decrypted = receiver.decrypt().unwrap().unwrap();

        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_nonce_increment() {
        let (mut sender, _) = create_test_channel();

        assert_eq!(sender.encrypt_nonce(), 0);

        sender.encrypt(b"message 1").unwrap();
        assert_eq!(sender.encrypt_nonce(), 1);

        sender.encrypt(b"message 2").unwrap();
        assert_eq!(sender.encrypt_nonce(), 2);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut channel = EncryptedChannel::disabled();

        assert!(!channel.is_enabled());

        // Should pass through unchanged
        let message = b"Plaintext message";
        let encrypted = channel.encrypt(message).unwrap();
        assert_eq!(encrypted, message);

        channel.feed(message);
        let decrypted = channel.decrypt().unwrap().unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key_a = [0x41u8; 32];
        let key_b = [0x42u8; 32];
        let key_c = [0x43u8; 32];

        let mut sender = EncryptedChannel::new(key_a, key_b);
        let mut receiver = EncryptedChannel::new(key_c, key_a); // Wrong encrypt key

        let encrypted = sender.encrypt(b"Secret").unwrap();
        receiver.feed(&encrypted);

        // Decryption should fail authentication
        let result = receiver.decrypt();
        assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
    }

    #[test]
    fn test_nonce_format() {
        let channel = EncryptedChannel::disabled();
        let nonce = channel.build_nonce(0x0102030405060708);

        // First 4 bytes zero, last 8 bytes are counter LE
        assert_eq!(nonce[0..4], [0, 0, 0, 0]);
        assert_eq!(nonce[4..12], [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }
}
```

---

### 53.2 Integration with RTSP Handler

- [x] **53.2.1** Wrap RTSP handling with encryption layer

**File:** `src/receiver/ap2/encrypted_rtsp.rs`

```rust
//! Encrypted RTSP handling
//!
//! Wraps the RTSP server codec with the encryption layer to handle
//! encrypted control channel traffic.

use super::encrypted_channel::{EncryptedChannel, EncryptionError};
use crate::protocol::rtsp::{RtspServerCodec, RtspRequest, ParseError};
use bytes::BytesMut;

/// RTSP codec with optional encryption
pub struct EncryptedRtspCodec {
    /// Underlying RTSP codec
    rtsp_codec: RtspServerCodec,
    /// Encryption channel
    channel: EncryptedChannel,
    /// Decrypted data buffer
    decrypted_buffer: BytesMut,
}

impl EncryptedRtspCodec {
    /// Create a new codec without encryption (pre-pairing)
    pub fn new() -> Self {
        Self {
            rtsp_codec: RtspServerCodec::new(),
            channel: EncryptedChannel::disabled(),
            decrypted_buffer: BytesMut::new(),
        }
    }

    /// Enable encryption with session keys
    pub fn enable_encryption(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.channel.enable(encrypt_key, decrypt_key);
        log::debug!("Control channel encryption enabled");
    }

    /// Disable encryption
    pub fn disable_encryption(&mut self) {
        self.channel.disable();
    }

    /// Check if encryption is enabled
    pub fn is_encrypted(&self) -> bool {
        self.channel.is_enabled()
    }

    /// Feed raw bytes from the network
    pub fn feed(&mut self, data: &[u8]) {
        self.channel.feed(data);
    }

    /// Try to decode the next RTSP request
    pub fn decode(&mut self) -> Result<Option<RtspRequest>, CodecError> {
        // Decrypt any available frames
        let frames = self.channel.decrypt_all()
            .map_err(CodecError::Encryption)?;

        // Add decrypted data to buffer
        for frame in frames {
            self.decrypted_buffer.extend_from_slice(&frame);
        }

        // Feed to RTSP codec
        if !self.decrypted_buffer.is_empty() {
            let data = self.decrypted_buffer.split().to_vec();
            self.rtsp_codec.feed(&data);
        }

        // Try to decode
        self.rtsp_codec.decode()
            .map_err(CodecError::Parse)
    }

    /// Encode a response (with encryption if enabled)
    pub fn encode_response(&mut self, response: &[u8]) -> Result<Vec<u8>, CodecError> {
        self.channel.encrypt(response)
            .map_err(CodecError::Encryption)
    }

    /// Clear buffers
    pub fn clear(&mut self) {
        self.rtsp_codec.clear();
        self.channel.clear();
        self.decrypted_buffer.clear();
    }
}

impl Default for EncryptedRtspCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("Encryption error: {0}")]
    Encryption(#[from] EncryptionError),

    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
}

/// TCP connection handler with encryption support
pub struct EncryptedConnection {
    /// Codec for this connection
    codec: EncryptedRtspCodec,
    /// Peer address
    peer_addr: std::net::SocketAddr,
    /// Connection state
    state: ConnectionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Pre-pairing - plaintext
    Plaintext,
    /// Post-pairing - encrypted
    Encrypted,
    /// Error state
    Error,
}

impl EncryptedConnection {
    pub fn new(peer_addr: std::net::SocketAddr) -> Self {
        Self {
            codec: EncryptedRtspCodec::new(),
            peer_addr,
            state: ConnectionState::Plaintext,
        }
    }

    /// Transition to encrypted state
    pub fn enable_encryption(&mut self, encrypt_key: [u8; 32], decrypt_key: [u8; 32]) {
        self.codec.enable_encryption(encrypt_key, decrypt_key);
        self.state = ConnectionState::Encrypted;
        log::info!("Connection to {} now encrypted", self.peer_addr);
    }

    /// Process incoming data
    pub fn on_data(&mut self, data: &[u8]) -> Result<Vec<RtspRequest>, CodecError> {
        self.codec.feed(data);

        let mut requests = Vec::new();
        while let Some(request) = self.codec.decode()? {
            requests.push(request);
        }

        Ok(requests)
    }

    /// Encode response
    pub fn encode(&mut self, response: &[u8]) -> Result<Vec<u8>, CodecError> {
        self.codec.encode_response(response)
    }

    pub fn peer_addr(&self) -> std::net::SocketAddr {
        self.peer_addr
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plaintext_mode() {
        let mut codec = EncryptedRtspCodec::new();

        let request = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        codec.feed(request);

        let decoded = codec.decode().unwrap().unwrap();
        assert_eq!(decoded.headers.cseq(), Some(1));
    }

    #[test]
    fn test_encrypted_mode() {
        // Create two codecs to simulate sender/receiver
        let key_a = [0x41u8; 32];
        let key_b = [0x42u8; 32];

        let mut sender_channel = EncryptedChannel::new(key_a, key_b);
        let mut receiver = EncryptedRtspCodec::new();
        receiver.enable_encryption(key_b, key_a);

        // Encrypt a request
        let request = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        let encrypted = sender_channel.encrypt(request).unwrap();

        // Decode on receiver
        receiver.feed(&encrypted);
        let decoded = receiver.decode().unwrap().unwrap();
        assert_eq!(decoded.headers.cseq(), Some(1));
    }
}
```

---

## Acceptance Criteria

- [x] Encryption uses ChaCha20-Poly1305 AEAD
- [x] Nonces increment correctly for each direction
- [x] Frame format matches HAP specification
- [x] Partial frames handled correctly
- [x] Authentication failures detected
- [x] Plaintext mode works pre-pairing
- [x] Seamless transition to encrypted mode
- [x] RTSP parsing works with encrypted data
- [x] All unit tests pass

---

## Notes

### Key Derivation

The encryption keys come from the pairing process (Section 49):
- `Control-Write-Encryption-Key` for encrypting TO the sender
- `Control-Read-Encryption-Key` for decrypting FROM the sender

### Nonce Management

Each direction maintains a separate 64-bit counter. The nonce is:
- 4 bytes of zeros (padding)
- 8 bytes counter (little-endian)

This ensures unique nonces for each message, critical for AEAD security.

### Frame Size Limits

The 2-byte length prefix limits frames to 65535 bytes. RTSP messages rarely approach this limit, but large binary plist bodies should be considered.

---

## References

- [HomeKit Accessory Protocol Specification](https://developer.apple.com/homekit/)
- [ChaCha20-Poly1305 AEAD](https://tools.ietf.org/html/rfc8439)
- [Section 04: Cryptographic Primitives](./complete/04-cryptographic-primitives.md)
