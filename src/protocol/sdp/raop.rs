//! RAOP-specific SDP parsing
//!
//! Extracts audio format parameters from RAOP ANNOUNCE SDP.

use super::{MediaDescription, SdpParseError, SessionDescription};
use crate::receiver::session::{AudioCodec, StreamParameters};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// ALAC format parameters from fmtp line
#[derive(Debug, Clone)]
pub struct AlacParameters {
    /// Frames per packet
    pub frames_per_packet: u32,
    /// Compatible version
    pub compatible_version: u8,
    /// Bits per sample
    pub bit_depth: u8,
    /// Rice history mult
    pub pb: u8,
    /// Rice initial history
    pub mb: u8,
    /// Rice limit
    pub kb: u8,
    /// Number of channels
    pub channels: u8,
    /// Max run
    pub max_run: u16,
    /// Max frame bytes
    pub max_frame_bytes: u32,
    /// Average bit rate
    pub avg_bit_rate: u32,
    /// Sample rate
    pub sample_rate: u32,
}

impl AlacParameters {
    /// Parse from fmtp attribute value
    /// Format: "96 352 0 16 40 10 14 2 255 0 0 44100"
    ///
    /// # Errors
    /// Returns `SdpParseError` if the fmtp string is invalid.
    pub fn parse(fmtp: &str) -> Result<Self, SdpParseError> {
        let parts: Vec<&str> = fmtp.split_whitespace().collect();

        // Determine offset based on whether payload type is present
        // 12 fields: payload type included (standard SDP)
        // 11 fields: payload type omitted (some RAOP implementations?)
        let offset = match parts.len() {
            12 => 1,
            11 => 0,
            n => {
                return Err(SdpParseError::InvalidAttribute(format!(
                    "ALAC fmtp needs 11 or 12 fields, got {n}: {fmtp}"
                )));
            }
        };

        Ok(AlacParameters {
            frames_per_packet: parts
                .get(offset)
                .and_then(|s| s.parse().ok())
                .unwrap_or(352),
            compatible_version: parts
                .get(offset + 1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            bit_depth: parts
                .get(offset + 2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(16),
            pb: parts
                .get(offset + 3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(40),
            mb: parts
                .get(offset + 4)
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            kb: parts
                .get(offset + 5)
                .and_then(|s| s.parse().ok())
                .unwrap_or(14),
            channels: parts
                .get(offset + 6)
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            max_run: parts
                .get(offset + 7)
                .and_then(|s| s.parse().ok())
                .unwrap_or(255),
            max_frame_bytes: parts
                .get(offset + 8)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            avg_bit_rate: parts
                .get(offset + 9)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            sample_rate: parts
                .get(offset + 10)
                .and_then(|s| s.parse().ok())
                .unwrap_or(44100),
        })
    }
}

/// AAC format parameters
#[derive(Debug, Clone)]
pub struct AacParameters {
    /// Sample rate
    pub sample_rate: u32,
    /// Channels
    pub channels: u8,
    /// AAC profile
    pub profile: AacProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacProfile {
    LowComplexity,
    EnhancedLowDelay,
}

/// Encryption parameters from SDP
#[derive(Debug, Clone)]
pub struct EncryptionParams {
    /// RSA-encrypted AES key (base64-decoded)
    pub encrypted_aes_key: Vec<u8>,
    /// AES IV (base64-decoded)
    pub aes_iv: [u8; 16],
}

/// Parse encryption parameters from SDP attributes
///
/// # Errors
/// Returns `SdpParseError` if required fields are missing or invalid (base64).
pub fn parse_encryption(
    media: &MediaDescription,
) -> Result<Option<EncryptionParams>, SdpParseError> {
    // Explicit type to help inference
    let encrypted_key: &String = match media.attributes.get("rsaaeskey") {
        Some(Some(key)) => key,
        Some(None) | None => return Ok(None),
    };

    let iv_str = media
        .attributes
        .get("aesiv")
        .and_then(|v: &Option<String>| v.as_deref())
        .ok_or(SdpParseError::MissingField("aesiv"))?;

    // Decode base64
    let encrypted_aes_key = BASE64
        .decode(encrypted_key.trim())
        .map_err(|_| SdpParseError::InvalidAttribute("Invalid base64 in rsaaeskey".to_string()))?;

    let iv_bytes = BASE64
        .decode(iv_str.trim())
        .map_err(|_| SdpParseError::InvalidAttribute("Invalid base64 in aesiv".to_string()))?;

    if iv_bytes.len() != 16 {
        return Err(SdpParseError::InvalidAttribute(format!(
            "AES IV must be 16 bytes, got {}",
            iv_bytes.len()
        )));
    }

    let mut aes_iv = [0u8; 16];
    aes_iv.copy_from_slice(&iv_bytes);

    Ok(Some(EncryptionParams {
        encrypted_aes_key,
        aes_iv,
    }))
}

/// Detect codec from rtpmap attribute
#[must_use]
pub fn detect_codec(media: &MediaDescription) -> Option<AudioCodec> {
    let rtpmap = media.attributes.get("rtpmap")?.as_deref()?;

    if rtpmap.contains("AppleLossless") {
        Some(AudioCodec::Alac)
    } else if rtpmap.contains("mpeg4-generic") || rtpmap.contains("MP4A-LATM") {
        // Check for AAC-ELD vs AAC-LC
        if rtpmap.contains("ELD") {
            Some(AudioCodec::AacEld)
        } else {
            Some(AudioCodec::AacLc)
        }
    } else if rtpmap.contains("L16") {
        Some(AudioCodec::Pcm)
    } else {
        None
    }
}

/// Extract stream parameters from SDP session
///
/// # Errors
/// Returns `SdpParseError` if required fields are missing or invalid.
pub fn extract_stream_parameters(
    sdp: &SessionDescription,
    rsa_private_key: Option<&[u8]>,
) -> Result<StreamParameters, SdpParseError> {
    let media = sdp
        .audio_media()
        .ok_or(SdpParseError::MissingField("audio media"))?;

    let codec = detect_codec(media).ok_or(SdpParseError::MissingField("rtpmap"))?;

    let (sample_rate, bits_per_sample, channels, frames_per_packet) = match codec {
        AudioCodec::Alac => {
            let fmtp = media
                .attributes
                .get("fmtp")
                .and_then(|v: &Option<String>| v.as_deref())
                .ok_or(SdpParseError::MissingField("fmtp"))?;
            let alac = AlacParameters::parse(fmtp)?;
            (
                alac.sample_rate,
                alac.bit_depth,
                alac.channels,
                alac.frames_per_packet,
            )
        }
        AudioCodec::Pcm | AudioCodec::AacLc | AudioCodec::AacEld => {
            // L16 and AAC defaults
            (44100, 16, 2, 352)
        }
    };

    // Parse encryption if present
    let encryption = parse_encryption(media)?;

    let (aes_key, aes_iv) = if let Some(enc) = encryption {
        // Decrypt AES key using RSA
        let key = if let Some(rsa_key) = rsa_private_key {
            Some(decrypt_aes_key(&enc.encrypted_aes_key, rsa_key)?)
        } else {
            None
        };
        (key, Some(enc.aes_iv))
    } else {
        (None, None)
    };

    // Parse min-latency if present
    let min_latency = media
        .attributes
        .get("min-latency")
        .and_then(|v: &Option<String>| v.as_ref())
        .and_then(|s: &String| s.parse().ok());

    Ok(StreamParameters {
        codec,
        sample_rate,
        bits_per_sample,
        channels,
        frames_per_packet,
        aes_key,
        aes_iv,
        min_latency,
    })
}

/// Decrypt AES key using RSA private key
fn decrypt_aes_key(encrypted: &[u8], rsa_private_key: &[u8]) -> Result<[u8; 16], SdpParseError> {
    #[cfg(feature = "raop")]
    {
        use rsa::pkcs8::DecodePrivateKey;
        use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

        // Parse RSA private key
        let private_key = RsaPrivateKey::from_pkcs8_der(rsa_private_key).map_err(|e| {
            SdpParseError::InvalidAttribute(format!("Invalid RSA key: {e}"))
        })?;

        // Decrypt using PKCS#1 v1.5
        let decrypted = private_key
            .decrypt(Pkcs1v15Encrypt, encrypted)
            .map_err(|e| {
                SdpParseError::InvalidAttribute(format!("RSA decrypt failed: {e}"))
            })?;

        if decrypted.len() != 16 {
            return Err(SdpParseError::InvalidAttribute(format!(
                "Decrypted AES key must be 16 bytes, got {}",
                decrypted.len()
            )));
        }

        let mut key = [0u8; 16];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }

    #[cfg(not(feature = "raop"))]
    {
        let _ = encrypted;
        let _ = rsa_private_key;
        Err(SdpParseError::InvalidAttribute(
            "RSA decryption requires 'raop' feature".to_string(),
        ))
    }
}
