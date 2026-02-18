//! RAOP-specific SDP parsing
//!
//! Extracts audio format parameters from RAOP ANNOUNCE SDP.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::{MediaDescription, SdpParseError, SessionDescription};
use crate::receiver::session::{AudioCodec, StreamParameters};

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
        fn parse_part<T: std::str::FromStr>(
            parts: &[&str],
            index: usize,
            name: &str,
        ) -> Result<T, SdpParseError> {
            let val_str = parts.get(index).ok_or_else(|| {
                SdpParseError::InvalidAttribute(format!("Missing field '{name}' at index {index}"))
            })?;
            val_str.parse().map_err(|_| {
                SdpParseError::InvalidAttribute(format!("Invalid value for '{name}': {val_str}"))
            })
        }

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
            frames_per_packet: parse_part(&parts, offset, "frames_per_packet")?,
            compatible_version: parse_part(&parts, offset + 1, "compatible_version")?,
            bit_depth: parse_part(&parts, offset + 2, "bit_depth")?,
            pb: parse_part(&parts, offset + 3, "pb")?,
            mb: parse_part(&parts, offset + 4, "mb")?,
            kb: parse_part(&parts, offset + 5, "kb")?,
            channels: parse_part(&parts, offset + 6, "channels")?,
            max_run: parse_part(&parts, offset + 7, "max_run")?,
            max_frame_bytes: parse_part(&parts, offset + 8, "max_frame_bytes")?,
            avg_bit_rate: parse_part(&parts, offset + 9, "avg_bit_rate")?,
            sample_rate: parse_part(&parts, offset + 10, "sample_rate")?,
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
        let private_key = RsaPrivateKey::from_pkcs8_der(rsa_private_key)
            .map_err(|e| SdpParseError::InvalidAttribute(format!("Invalid RSA key: {e}")))?;

        // Decrypt using PKCS#1 v1.5
        let decrypted = private_key
            .decrypt(Pkcs1v15Encrypt, encrypted)
            .map_err(|e| SdpParseError::InvalidAttribute(format!("RSA decrypt failed: {e}")))?;

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
