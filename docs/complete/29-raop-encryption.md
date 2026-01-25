# Section 29: RAOP Audio Encryption

## Dependencies
- **Section 04**: Cryptographic Primitives (must be complete)
- **Section 26**: RSA Authentication (must be complete)
- **Section 28**: RTP Audio Streaming (recommended)

## Overview

RAOP uses AES-128 encryption in Counter (CTR) mode for audio payload protection. The encryption key is exchanged using RSA-OAEP during the ANNOUNCE phase. This section details the encryption scheme and its integration with the audio streaming pipeline.

## Encryption Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Key Exchange (ANNOUNCE)                       │
│                                                                  │
│  Client                                           Server         │
│    │                                                │            │
│    │  1. Generate random AES-128 key              │            │
│    │  2. Generate random 128-bit IV               │            │
│    │  3. Encrypt AES key with RSA-OAEP            │            │
│    │  4. Send rsaaeskey + aesiv in SDP            │            │
│    │─────────────────────────────────────────────>│            │
│    │                                                │            │
│    │                        5. Decrypt AES key with RSA private  │
│    │                        6. Store key + IV for session        │
│    │<─────────────────────────────────────────────│            │
│    │             200 OK                            │            │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    Audio Encryption (RTP)                        │
│                                                                  │
│    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐     │
│    │  PCM/ALAC    │───>│  AES-128-CTR │───>│  RTP Packet  │     │
│    │  Audio Data  │    │  Encrypt     │    │              │     │
│    └──────────────┘    └──────────────┘    └──────────────┘     │
│                              │                                   │
│                    Key + IV + Counter                            │
└─────────────────────────────────────────────────────────────────┘
```

## Objectives

- Implement AES-128-CTR encryption for audio payloads
- Handle proper counter management for streaming
- Integrate encryption with RTP packet generation
- Support unencrypted mode for compatible devices

---

## Tasks

### 29.1 AES-CTR Encryption Module

- [x] **29.1.1** Implement RAOP-specific AES-CTR wrapper

**File:** `src/protocol/raop/encryption.rs`

```rust
//! RAOP audio encryption using AES-128-CTR

use crate::protocol::crypto::{Aes128Ctr, CryptoError};

/// AES key size (128 bits)
pub const AES_KEY_SIZE: usize = 16;
/// AES IV size (128 bits)
pub const AES_IV_SIZE: usize = 16;
/// Audio frame size (352 samples * 4 bytes)
pub const FRAME_SIZE: usize = 352 * 4;

/// RAOP audio encryptor
///
/// Handles AES-128-CTR encryption for audio packets.
/// The counter is based on the IV and packet sequence/timestamp.
pub struct RaopEncryptor {
    /// AES encryption key
    key: [u8; AES_KEY_SIZE],
    /// Base initialization vector
    iv: [u8; AES_IV_SIZE],
    /// Whether encryption is enabled
    enabled: bool,
}

impl RaopEncryptor {
    /// Create a new encryptor with given key and IV
    pub fn new(key: [u8; AES_KEY_SIZE], iv: [u8; AES_IV_SIZE]) -> Self {
        Self {
            key,
            iv,
            enabled: true,
        }
    }

    /// Create an encryptor with encryption disabled
    pub fn disabled() -> Self {
        Self {
            key: [0; AES_KEY_SIZE],
            iv: [0; AES_IV_SIZE],
            enabled: false,
        }
    }

    /// Check if encryption is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Encrypt audio data for a packet
    ///
    /// # Arguments
    /// * `audio_data` - Raw audio bytes (PCM or encoded)
    /// * `packet_index` - Packet index for counter derivation
    ///
    /// # Returns
    /// Encrypted audio data
    pub fn encrypt(&self, audio_data: &[u8], packet_index: u64) -> Result<Vec<u8>, CryptoError> {
        if !self.enabled {
            return Ok(audio_data.to_vec());
        }

        let mut cipher = Aes128Ctr::new(&self.key, &self.iv)?;

        // Seek to the correct position in the keystream
        // Each packet uses FRAME_SIZE bytes of keystream
        cipher.seek(packet_index * FRAME_SIZE as u64);

        let mut output = audio_data.to_vec();
        cipher.apply_keystream(&mut output);

        Ok(output)
    }

    /// Encrypt audio data in place
    pub fn encrypt_in_place(
        &self,
        audio_data: &mut [u8],
        packet_index: u64,
    ) -> Result<(), CryptoError> {
        if !self.enabled {
            return Ok(());
        }

        let mut cipher = Aes128Ctr::new(&self.key, &self.iv)?;
        cipher.seek(packet_index * FRAME_SIZE as u64);
        cipher.apply_keystream(audio_data);

        Ok(())
    }

    /// Get a reference to the key (for session info)
    pub fn key(&self) -> &[u8; AES_KEY_SIZE] {
        &self.key
    }

    /// Get a reference to the IV
    pub fn iv(&self) -> &[u8; AES_IV_SIZE] {
        &self.iv
    }
}

impl Drop for RaopEncryptor {
    fn drop(&mut self) {
        // Zeroize sensitive data
        self.key.iter_mut().for_each(|b| *b = 0);
        self.iv.iter_mut().for_each(|b| *b = 0);
    }
}

/// RAOP audio decryptor (for receiver/testing)
pub struct RaopDecryptor {
    /// AES decryption key
    key: [u8; AES_KEY_SIZE],
    /// Base initialization vector
    iv: [u8; AES_IV_SIZE],
    /// Whether encryption is enabled
    enabled: bool,
}

impl RaopDecryptor {
    /// Create a new decryptor with given key and IV
    pub fn new(key: [u8; AES_KEY_SIZE], iv: [u8; AES_IV_SIZE]) -> Self {
        Self {
            key,
            iv,
            enabled: true,
        }
    }

    /// Decrypt audio data from a packet
    pub fn decrypt(&self, audio_data: &[u8], packet_index: u64) -> Result<Vec<u8>, CryptoError> {
        if !self.enabled {
            return Ok(audio_data.to_vec());
        }

        // AES-CTR decryption is the same as encryption
        let mut cipher = Aes128Ctr::new(&self.key, &self.iv)?;
        cipher.seek(packet_index * FRAME_SIZE as u64);

        let mut output = audio_data.to_vec();
        cipher.apply_keystream(&mut output);

        Ok(output)
    }
}

impl Drop for RaopDecryptor {
    fn drop(&mut self) {
        self.key.iter_mut().for_each(|b| *b = 0);
        self.iv.iter_mut().for_each(|b| *b = 0);
    }
}
```

---

### 29.2 Key Generation and Exchange

- [x] **29.2.1** Implement secure key generation

**File:** `src/protocol/raop/encryption.rs` (continued)

```rust
/// Generate random AES key and IV
pub fn generate_encryption_keys() -> Result<([u8; AES_KEY_SIZE], [u8; AES_IV_SIZE]), CryptoError> {
    use rand::RngCore;

    let mut key = [0u8; AES_KEY_SIZE];
    let mut iv = [0u8; AES_IV_SIZE];

    let mut rng = rand::thread_rng();
    rng.try_fill_bytes(&mut key)
        .map_err(|_| CryptoError::RngError)?;
    rng.try_fill_bytes(&mut iv)
        .map_err(|_| CryptoError::RngError)?;

    Ok((key, iv))
}

/// RSA-wrapped session keys for SDP
pub struct WrappedSessionKeys {
    /// RSA-OAEP encrypted AES key (Base64)
    pub rsaaeskey: String,
    /// AES IV (Base64)
    pub aesiv: String,
    /// Plain AES key (for local encryption)
    key: [u8; AES_KEY_SIZE],
    /// Plain AES IV
    iv: [u8; AES_IV_SIZE],
}

impl WrappedSessionKeys {
    /// Generate new session keys and wrap with RSA
    pub fn generate() -> Result<Self, CryptoError> {
        use crate::protocol::crypto::rsa::AppleRsaPublicKey;
        use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD as BASE64};

        let (key, iv) = generate_encryption_keys()?;

        // Encrypt AES key with Apple's RSA public key
        let public_key = AppleRsaPublicKey::load()?;
        let encrypted_key = public_key.encrypt_oaep(&key)?;

        Ok(Self {
            rsaaeskey: BASE64.encode(&encrypted_key),
            aesiv: BASE64.encode(&iv),
            key,
            iv,
        })
    }

    /// Create encryptor from these keys
    pub fn encryptor(&self) -> RaopEncryptor {
        RaopEncryptor::new(self.key, self.iv)
    }

    /// Get plain AES key (for debugging/testing)
    #[cfg(test)]
    pub fn plain_key(&self) -> &[u8; AES_KEY_SIZE] {
        &self.key
    }

    /// Get plain IV (for debugging/testing)
    #[cfg(test)]
    pub fn plain_iv(&self) -> &[u8; AES_IV_SIZE] {
        &self.iv
    }
}

impl Drop for WrappedSessionKeys {
    fn drop(&mut self) {
        self.key.iter_mut().for_each(|b| *b = 0);
        self.iv.iter_mut().for_each(|b| *b = 0);
    }
}

/// Parse received session keys (receiver side)
pub fn parse_wrapped_keys(
    rsaaeskey_b64: &str,
    aesiv_b64: &str,
    private_key: &crate::protocol::crypto::rsa::RaopRsaPrivateKey,
) -> Result<(RaopDecryptor, [u8; AES_KEY_SIZE], [u8; AES_IV_SIZE]), CryptoError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD as BASE64};

    // Decode Base64
    let encrypted_key = BASE64.decode(rsaaeskey_b64.trim())
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid base64: {}", e)))?;

    let iv_bytes = BASE64.decode(aesiv_b64.trim())
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid base64: {}", e)))?;

    // Decrypt AES key
    let key_bytes = private_key.decrypt_oaep(&encrypted_key)?;

    if key_bytes.len() != AES_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength {
            expected: AES_KEY_SIZE,
            actual: key_bytes.len(),
        });
    }

    if iv_bytes.len() != AES_IV_SIZE {
        return Err(CryptoError::InvalidKeyLength {
            expected: AES_IV_SIZE,
            actual: iv_bytes.len(),
        });
    }

    let mut key = [0u8; AES_KEY_SIZE];
    let mut iv = [0u8; AES_IV_SIZE];
    key.copy_from_slice(&key_bytes);
    iv.copy_from_slice(&iv_bytes);

    let decryptor = RaopDecryptor::new(key, iv);

    Ok((decryptor, key, iv))
}
```

---

### 29.3 ALAC Encryption Integration

- [x] **29.3.1** Integrate encryption with audio encoding

**File:** `src/audio/raop_encoder.rs`

```rust
//! RAOP audio encoding with encryption

use crate::protocol::raop::encryption::{RaopEncryptor, FRAME_SIZE};
use crate::protocol::crypto::CryptoError;

/// RAOP audio encoder with encryption
pub struct RaopAudioEncoder {
    /// Audio encryptor
    encryptor: RaopEncryptor,
    /// Current packet index
    packet_index: u64,
    /// Sample rate
    sample_rate: u32,
    /// Samples per frame
    samples_per_frame: u32,
}

impl RaopAudioEncoder {
    /// Samples per ALAC frame
    pub const ALAC_FRAME_SAMPLES: u32 = 352;

    /// Create new encoder
    pub fn new(encryptor: RaopEncryptor) -> Self {
        Self {
            encryptor,
            packet_index: 0,
            sample_rate: 44100,
            samples_per_frame: Self::ALAC_FRAME_SAMPLES,
        }
    }

    /// Encode and encrypt a frame of audio
    ///
    /// # Arguments
    /// * `pcm_samples` - 16-bit stereo PCM samples (interleaved L/R)
    ///
    /// # Returns
    /// Encrypted audio data ready for RTP packet
    pub fn encode_frame(&mut self, pcm_samples: &[i16]) -> Result<Vec<u8>, AudioEncodeError> {
        // Convert to bytes
        let mut audio_bytes = Vec::with_capacity(pcm_samples.len() * 2);
        for sample in pcm_samples {
            audio_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        // For PCM mode, use raw bytes
        // For ALAC mode, would encode here

        // Encrypt
        let encrypted = self.encryptor.encrypt(&audio_bytes, self.packet_index)
            .map_err(|e| AudioEncodeError::Encryption(e.to_string()))?;

        self.packet_index += 1;

        Ok(encrypted)
    }

    /// Encode raw audio bytes
    pub fn encode_raw(&mut self, audio_data: &[u8]) -> Result<Vec<u8>, AudioEncodeError> {
        let encrypted = self.encryptor.encrypt(audio_data, self.packet_index)
            .map_err(|e| AudioEncodeError::Encryption(e.to_string()))?;

        self.packet_index += 1;

        Ok(encrypted)
    }

    /// Reset packet index (after flush)
    pub fn reset(&mut self) {
        self.packet_index = 0;
    }

    /// Get current packet index
    pub fn packet_index(&self) -> u64 {
        self.packet_index
    }
}

/// Audio encoding errors
#[derive(Debug, thiserror::Error)]
pub enum AudioEncodeError {
    #[error("encryption error: {0}")]
    Encryption(String),
    #[error("invalid frame size: expected {expected}, got {actual}")]
    InvalidFrameSize { expected: usize, actual: usize },
    #[error("encoding error: {0}")]
    Encoding(String),
}
```

---

### 29.4 Unencrypted Mode

- [x] **29.4.1** Support devices that accept unencrypted audio

**File:** `src/protocol/raop/encryption.rs` (continued)

```rust
/// Encryption mode for RAOP session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    /// No encryption (et=0 in TXT records)
    None,
    /// RSA encryption (et=1)
    Rsa,
    /// FairPlay encryption (et=3, not supported)
    FairPlay,
    /// MFi-SAP encryption (et=4, not supported)
    MfiSap,
    /// FairPlay SAPv2.5 (et=5, not supported)
    FairPlaySap25,
}

impl EncryptionMode {
    /// Parse from TXT record value
    pub fn from_txt(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Rsa),
            3 => Some(Self::FairPlay),
            4 => Some(Self::MfiSap),
            5 => Some(Self::FairPlaySap25),
            _ => None,
        }
    }

    /// Check if this mode is supported
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::None | Self::Rsa)
    }
}

/// Session encryption configuration
pub struct EncryptionConfig {
    /// Encryption mode
    pub mode: EncryptionMode,
    /// Encryptor (if encryption enabled)
    encryptor: Option<RaopEncryptor>,
    /// Session keys (if encryption enabled)
    keys: Option<WrappedSessionKeys>,
}

impl EncryptionConfig {
    /// Create unencrypted configuration
    pub fn unencrypted() -> Self {
        Self {
            mode: EncryptionMode::None,
            encryptor: Some(RaopEncryptor::disabled()),
            keys: None,
        }
    }

    /// Create RSA-encrypted configuration
    pub fn rsa() -> Result<Self, CryptoError> {
        let keys = WrappedSessionKeys::generate()?;
        let encryptor = keys.encryptor();

        Ok(Self {
            mode: EncryptionMode::Rsa,
            encryptor: Some(encryptor),
            keys: Some(keys),
        })
    }

    /// Get encryptor
    pub fn encryptor(&self) -> Option<&RaopEncryptor> {
        self.encryptor.as_ref()
    }

    /// Get session keys for SDP
    pub fn session_keys(&self) -> Option<&WrappedSessionKeys> {
        self.keys.as_ref()
    }

    /// Check if encryption is active
    pub fn is_encrypted(&self) -> bool {
        self.mode != EncryptionMode::None
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/raop/encryption.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; AES_KEY_SIZE];
        let iv = [0x00u8; AES_IV_SIZE];

        let encryptor = RaopEncryptor::new(key, iv);
        let decryptor = RaopDecryptor::new(key, iv);

        let original = vec![0xAA; FRAME_SIZE];
        let packet_index = 0;

        let encrypted = encryptor.encrypt(&original, packet_index).unwrap();
        assert_ne!(encrypted, original);

        let decrypted = decryptor.decrypt(&encrypted, packet_index).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_different_packets_different_ciphertext() {
        let key = [0x42u8; AES_KEY_SIZE];
        let iv = [0x00u8; AES_IV_SIZE];

        let encryptor = RaopEncryptor::new(key, iv);

        let data = vec![0xAA; FRAME_SIZE];

        let encrypted1 = encryptor.encrypt(&data, 0).unwrap();
        let encrypted2 = encryptor.encrypt(&data, 1).unwrap();

        // Same plaintext, different packet index -> different ciphertext
        assert_ne!(encrypted1, encrypted2);
    }

    #[test]
    fn test_disabled_encryption() {
        let encryptor = RaopEncryptor::disabled();

        let data = vec![0xAA; 100];
        let encrypted = encryptor.encrypt(&data, 0).unwrap();

        // Should be unchanged
        assert_eq!(encrypted, data);
    }

    #[test]
    fn test_key_generation() {
        let (key1, iv1) = generate_encryption_keys().unwrap();
        let (key2, iv2) = generate_encryption_keys().unwrap();

        // Should be different each time
        assert_ne!(key1, key2);
        assert_ne!(iv1, iv2);

        // Should be correct size
        assert_eq!(key1.len(), AES_KEY_SIZE);
        assert_eq!(iv1.len(), AES_IV_SIZE);
    }

    #[test]
    fn test_encrypt_in_place() {
        let key = [0x42u8; AES_KEY_SIZE];
        let iv = [0x00u8; AES_IV_SIZE];

        let encryptor = RaopEncryptor::new(key, iv);
        let decryptor = RaopDecryptor::new(key, iv);

        let original = vec![0xAA; FRAME_SIZE];
        let mut data = original.clone();

        encryptor.encrypt_in_place(&mut data, 0).unwrap();
        assert_ne!(data, original);

        let decrypted = decryptor.decrypt(&data, 0).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_encryption_mode_parsing() {
        assert_eq!(EncryptionMode::from_txt(0), Some(EncryptionMode::None));
        assert_eq!(EncryptionMode::from_txt(1), Some(EncryptionMode::Rsa));
        assert_eq!(EncryptionMode::from_txt(3), Some(EncryptionMode::FairPlay));
        assert_eq!(EncryptionMode::from_txt(99), None);

        assert!(EncryptionMode::None.is_supported());
        assert!(EncryptionMode::Rsa.is_supported());
        assert!(!EncryptionMode::FairPlay.is_supported());
    }

    #[test]
    fn test_sequential_packet_encryption() {
        let key = [0x42u8; AES_KEY_SIZE];
        let iv = [0x00u8; AES_IV_SIZE];

        let encryptor = RaopEncryptor::new(key, iv);
        let decryptor = RaopDecryptor::new(key, iv);

        // Simulate streaming multiple packets
        for i in 0..10u64 {
            let data = vec![(i & 0xFF) as u8; FRAME_SIZE];
            let encrypted = encryptor.encrypt(&data, i).unwrap();
            let decrypted = decryptor.decrypt(&encrypted, i).unwrap();
            assert_eq!(decrypted, data);
        }
    }
}
```

---

## Integration Tests

### Test: Full encryption flow simulation

```rust
// tests/raop_encryption_integration.rs

use airplay2_rs::protocol::raop::encryption::{
    RaopEncryptor, RaopDecryptor, WrappedSessionKeys,
    EncryptionConfig, EncryptionMode, FRAME_SIZE,
};
use airplay2_rs::protocol::crypto::rsa::RaopRsaPrivateKey;

#[test]
fn test_key_exchange_simulation() {
    // Server generates RSA key pair
    let server_key = RaopRsaPrivateKey::generate().unwrap();

    // Client generates session keys (would use Apple public key normally)
    let (client_key, client_iv) = airplay2_rs::protocol::raop::encryption::generate_encryption_keys().unwrap();

    // Client encrypts AES key with server's public key
    use rsa::Oaep;
    use sha1::Sha1;
    use rand::rngs::OsRng;

    let public = server_key.public_key();
    let padding = Oaep::new::<Sha1>();
    let encrypted_key = public.encrypt(&mut OsRng, padding, &client_key).unwrap();

    // Server decrypts to get AES key
    let decrypted_key = server_key.decrypt_oaep(&encrypted_key).unwrap();

    assert_eq!(decrypted_key, client_key);

    // Both sides can now encrypt/decrypt
    let client_encryptor = RaopEncryptor::new(client_key, client_iv);
    let server_decryptor = RaopDecryptor::new(
        client_key.try_into().unwrap(),
        client_iv,
    );

    let test_audio = vec![0x55u8; FRAME_SIZE];
    let encrypted = client_encryptor.encrypt(&test_audio, 0).unwrap();
    let decrypted = server_decryptor.decrypt(&encrypted, 0).unwrap();

    assert_eq!(decrypted, test_audio);
}

#[test]
fn test_unencrypted_mode() {
    let config = EncryptionConfig::unencrypted();

    assert_eq!(config.mode, EncryptionMode::None);
    assert!(!config.is_encrypted());

    let encryptor = config.encryptor().unwrap();
    assert!(!encryptor.is_enabled());

    let data = vec![0xAA; 100];
    let result = encryptor.encrypt(&data, 0).unwrap();
    assert_eq!(result, data);
}
```

---

## Acceptance Criteria

- [x] AES-128-CTR encryption works correctly
- [x] Different packets produce different ciphertext
- [x] Decrypt correctly reverses encryption
- [x] Key generation produces random keys
- [x] RSA key wrapping integrates with key exchange
- [x] Unencrypted mode passes data through unchanged
- [x] Keys are zeroized on drop
- [x] Sequential packet encryption works correctly
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- The AES counter is derived from packet index, not RTP sequence number
- Some implementations may use different counter derivation methods
- FairPlay encryption is not supported (Apple DRM)
- MFi-SAP encryption requires licensed hardware
- Consider adding AEAD (AES-GCM) for integrity checking in future
