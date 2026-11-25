//! RAOP audio encryption: AES-128-CBC, IV reset for every packet.
//!
//! Each RTP audio payload is encrypted on its own with the session key and the
//! session IV. Only the leading whole 16-byte blocks are encrypted; a trailing
//! partial block is sent in the clear. This is what Shairport Sync
//! (`player.c`), `OwnTone` and `AirPort` Express receivers expect, and what
//! `fuzz/fuzz_targets/raop_audio_vs_shairport.rs` checks.

use aes::Aes128;
use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};

use crate::protocol::crypto::CryptoError;
use crate::protocol::raop::key_exchange::RaopSessionKeys;

/// AES key size (128 bits)
pub const AES_KEY_SIZE: usize = 16;
/// AES IV size (128 bits)
pub const AES_IV_SIZE: usize = 16;
/// Audio frame size (352 samples * 4 bytes)
pub const FRAME_SIZE: usize = 352 * 4;

/// Encrypt one RAOP audio payload in place (AES-128-CBC, whole blocks only).
///
/// # Panics
///
/// Never: `chunks_exact_mut(16)` only yields 16-byte chunks.
pub fn cbc_encrypt_payload(key: &[u8; AES_KEY_SIZE], iv: &[u8; AES_IV_SIZE], data: &mut [u8]) {
    let cipher = Aes128::new(key.into());
    let mut prev = *iv;
    for chunk in data.chunks_exact_mut(16) {
        for (b, p) in chunk.iter_mut().zip(prev.iter()) {
            *b ^= *p;
        }
        let mut block = aes::Block::try_from(&*chunk).expect("16-byte chunk");
        cipher.encrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        prev.copy_from_slice(chunk);
    }
}

/// Decrypt one RAOP audio payload in place (AES-128-CBC, whole blocks only).
///
/// # Panics
///
/// Never: `chunks_exact_mut(16)` only yields 16-byte chunks.
pub fn cbc_decrypt_payload(key: &[u8; AES_KEY_SIZE], iv: &[u8; AES_IV_SIZE], data: &mut [u8]) {
    let cipher = Aes128::new(key.into());
    let mut prev = *iv;
    for chunk in data.chunks_exact_mut(16) {
        let ciphertext: [u8; 16] = (&*chunk).try_into().expect("16-byte chunk");
        let mut block = aes::Block::from(ciphertext);
        cipher.decrypt_block(&mut block);
        for (b, p) in block.iter_mut().zip(prev.iter()) {
            *b ^= *p;
        }
        chunk.copy_from_slice(&block);
        prev = ciphertext;
    }
}

/// RAOP audio encryptor
///
/// Encrypts each audio payload independently with AES-128-CBC; see the module
/// docs.
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
    #[must_use]
    pub fn new(key: [u8; AES_KEY_SIZE], iv: [u8; AES_IV_SIZE]) -> Self {
        Self {
            key,
            iv,
            enabled: true,
        }
    }

    /// Create an encryptor with encryption disabled
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            key: [0; AES_KEY_SIZE],
            iv: [0; AES_IV_SIZE],
            enabled: false,
        }
    }

    /// Check if encryption is enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Encrypt audio data for a packet
    ///
    /// # Arguments
    /// * `audio_data` - Raw audio bytes (PCM or encoded)
    /// * `packet_index` - Unused: RAOP restarts CBC from the session IV for every packet. Kept for
    ///   API compatibility.
    ///
    /// # Returns
    /// Encrypted audio data
    ///
    /// # Errors
    /// Never fails; the `Result` is kept for API compatibility.
    pub fn encrypt(&self, audio_data: &[u8], packet_index: u64) -> Result<Vec<u8>, CryptoError> {
        let mut output = audio_data.to_vec();
        self.encrypt_in_place(&mut output, packet_index)?;
        Ok(output)
    }

    /// Encrypt audio data in place
    ///
    /// # Errors
    /// Returns `CryptoError` if encryption fails
    pub fn encrypt_in_place(
        &self,
        audio_data: &mut [u8],
        _packet_index: u64,
    ) -> Result<(), CryptoError> {
        if self.enabled {
            cbc_encrypt_payload(&self.key, &self.iv, audio_data);
        }
        Ok(())
    }

    /// Get a reference to the key (for session info)
    #[must_use]
    pub fn key(&self) -> &[u8; AES_KEY_SIZE] {
        &self.key
    }

    /// Get a reference to the IV
    #[must_use]
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
    #[must_use]
    pub fn new(key: [u8; AES_KEY_SIZE], iv: [u8; AES_IV_SIZE]) -> Self {
        Self {
            key,
            iv,
            enabled: true,
        }
    }

    /// Decrypt audio data from a packet
    ///
    /// # Errors
    /// Returns `CryptoError` if decryption fails
    pub fn decrypt(&self, audio_data: &[u8], _packet_index: u64) -> Result<Vec<u8>, CryptoError> {
        let mut output = audio_data.to_vec();
        if self.enabled {
            cbc_decrypt_payload(&self.key, &self.iv, &mut output);
        }
        Ok(output)
    }
}

impl Drop for RaopDecryptor {
    fn drop(&mut self) {
        self.key.iter_mut().for_each(|b| *b = 0);
        self.iv.iter_mut().for_each(|b| *b = 0);
    }
}

/// Encryption mode for RAOP session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    /// No encryption (et=0 in TXT records)
    None,
    /// RSA encryption (et=1)
    Rsa,
    /// `FairPlay` encryption (et=3, not supported)
    FairPlay,
    /// MFi-SAP encryption (et=4, not supported)
    MfiSap,
    /// `FairPlay` SAPv2.5 (et=5, not supported)
    FairPlaySap25,
}

impl EncryptionMode {
    /// Parse from TXT record value
    #[must_use]
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
    #[must_use]
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
    keys: Option<RaopSessionKeys>,
}

impl EncryptionConfig {
    /// Create unencrypted configuration
    #[must_use]
    pub fn unencrypted() -> Self {
        Self {
            mode: EncryptionMode::None,
            encryptor: Some(RaopEncryptor::disabled()),
            keys: None,
        }
    }

    /// Create RSA-encrypted configuration
    ///
    /// # Errors
    /// Returns `CryptoError` if key generation fails
    pub fn rsa() -> Result<Self, CryptoError> {
        let keys = RaopSessionKeys::generate()?;
        let encryptor = RaopEncryptor::new(*keys.aes_key(), *keys.aes_iv());

        Ok(Self {
            mode: EncryptionMode::Rsa,
            encryptor: Some(encryptor),
            keys: Some(keys),
        })
    }

    /// Get encryptor
    #[must_use]
    pub fn encryptor(&self) -> Option<&RaopEncryptor> {
        self.encryptor.as_ref()
    }

    /// Get session keys for SDP
    #[must_use]
    pub fn session_keys(&self) -> Option<&RaopSessionKeys> {
        self.keys.as_ref()
    }

    /// Check if encryption is active
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.mode != EncryptionMode::None
    }
}
