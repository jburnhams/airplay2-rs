//! RAOP audio encryption using AES-128-CTR

use crate::protocol::crypto::{Aes128Ctr, CryptoError};
use crate::protocol::raop::key_exchange::RaopSessionKeys;

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
    /// * `packet_index` - Packet index for counter derivation
    ///
    /// # Returns
    /// Encrypted audio data
    ///
    /// # Errors
    /// Returns `CryptoError` if encryption fails
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
    ///
    /// # Errors
    /// Returns `CryptoError` if encryption fails
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
