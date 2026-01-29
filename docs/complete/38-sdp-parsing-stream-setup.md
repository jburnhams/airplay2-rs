# Section 38: SDP Parsing & Stream Setup

## Dependencies
- **Section 36**: RTSP Server (Sans-IO) (ANNOUNCE handling)
- **Section 37**: Session Management (StreamParameters storage)
- **Section 04**: Crypto Primitives (RSA decryption for AES key)

## Overview

This section implements SDP (Session Description Protocol) parsing for the receiver. When an AirPlay sender connects, it sends an ANNOUNCE request containing an SDP body that describes the audio stream:

- Codec type (PCM, ALAC, AAC)
- Audio format parameters (sample rate, channels, bits)
- Encryption keys (RSA-encrypted AES key and IV)
- Latency requirements

The receiver must parse this SDP to configure the audio pipeline correctly.

## Objectives

- Parse SDP bodies from ANNOUNCE requests
- Extract codec and format parameters from `a=fmtp:` lines
- Decrypt AES key from `a=rsaaeskey:` using RSA private key
- Decode AES IV from `a=aesiv:`
- Map SDP parameters to internal StreamParameters
- Handle multiple codec types (ALAC, AAC, PCM)

---

## Tasks

### 38.1 SDP Parser

- [x] **38.1.1** Implement SDP line parser

**File:** `src/protocol/sdp/parser.rs`

```rust
//! SDP parser for RAOP ANNOUNCE bodies
//!
//! Parses Session Description Protocol content to extract
//! audio stream parameters.

use std::collections::HashMap;

/// Parsed SDP session
#[derive(Debug, Clone)]
pub struct SdpSession {
    /// Session version (v=)
    pub version: u8,
    /// Origin (o=)
    pub origin: Option<SdpOrigin>,
    /// Session name (s=)
    pub session_name: String,
    /// Connection info (c=)
    pub connection: Option<SdpConnection>,
    /// Time description (t=)
    pub timing: Option<(u64, u64)>,
    /// Media descriptions (m=)
    pub media: Vec<SdpMedia>,
    /// Session-level attributes (a=)
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SdpOrigin {
    pub username: String,
    pub session_id: String,
    pub session_version: String,
    pub net_type: String,
    pub addr_type: String,
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct SdpConnection {
    pub net_type: String,
    pub addr_type: String,
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct SdpMedia {
    pub media_type: String,
    pub port: u16,
    pub protocol: String,
    pub formats: Vec<String>,
    pub attributes: HashMap<String, String>,
}

/// SDP parsing errors
#[derive(Debug, thiserror::Error)]
pub enum SdpParseError {
    #[error("Invalid SDP line: {0}")]
    InvalidLine(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Invalid origin line: {0}")]
    InvalidOrigin(String),

    #[error("Invalid media line: {0}")]
    InvalidMedia(String),

    #[error("Invalid fmtp: {0}")]
    InvalidFmtp(String),
}

impl SdpSession {
    /// Parse SDP from string
    pub fn parse(sdp: &str) -> Result<Self, SdpParseError> {
        let mut version = 0;
        let mut origin = None;
        let mut session_name = String::new();
        let mut connection = None;
        let mut timing = None;
        let mut media = Vec::new();
        let mut session_attributes = HashMap::new();
        let mut current_media: Option<SdpMedia> = None;

        for line in sdp.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.len() < 2 || line.chars().nth(1) != Some('=') {
                continue;  // Skip invalid lines
            }

            let line_type = line.chars().next().unwrap();
            let value = &line[2..];

            match line_type {
                'v' => {
                    version = value.parse()
                        .map_err(|_| SdpParseError::InvalidVersion(value.to_string()))?;
                }
                'o' => {
                    origin = Some(Self::parse_origin(value)?);
                }
                's' => {
                    session_name = value.to_string();
                }
                'c' => {
                    connection = Some(Self::parse_connection(value)?);
                }
                't' => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let start = parts[0].parse().unwrap_or(0);
                        let stop = parts[1].parse().unwrap_or(0);
                        timing = Some((start, stop));
                    }
                }
                'm' => {
                    // Save previous media block
                    if let Some(m) = current_media.take() {
                        media.push(m);
                    }
                    current_media = Some(Self::parse_media(value)?);
                }
                'a' => {
                    let (name, attr_value) = Self::parse_attribute(value);

                    if let Some(ref mut m) = current_media {
                        m.attributes.insert(name, attr_value);
                    } else {
                        session_attributes.insert(name, attr_value);
                    }
                }
                _ => {
                    // Ignore unknown line types
                }
            }
        }

        // Save last media block
        if let Some(m) = current_media {
            media.push(m);
        }

        Ok(SdpSession {
            version,
            origin,
            session_name,
            connection,
            timing,
            media,
            attributes: session_attributes,
        })
    }

    fn parse_origin(value: &str) -> Result<SdpOrigin, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 6 {
            return Err(SdpParseError::InvalidOrigin(value.to_string()));
        }

        Ok(SdpOrigin {
            username: parts[0].to_string(),
            session_id: parts[1].to_string(),
            session_version: parts[2].to_string(),
            net_type: parts[3].to_string(),
            addr_type: parts[4].to_string(),
            address: parts[5].to_string(),
        })
    }

    fn parse_connection(value: &str) -> Result<SdpConnection, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(SdpParseError::InvalidLine(value.to_string()));
        }

        Ok(SdpConnection {
            net_type: parts[0].to_string(),
            addr_type: parts[1].to_string(),
            address: parts[2].to_string(),
        })
    }

    fn parse_media(value: &str) -> Result<SdpMedia, SdpParseError> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(SdpParseError::InvalidMedia(value.to_string()));
        }

        Ok(SdpMedia {
            media_type: parts[0].to_string(),
            port: parts[1].parse().unwrap_or(0),
            protocol: parts[2].to_string(),
            formats: parts[3..].iter().map(|s| s.to_string()).collect(),
            attributes: HashMap::new(),
        })
    }

    fn parse_attribute(value: &str) -> (String, String) {
        if let Some(pos) = value.find(':') {
            (value[..pos].to_string(), value[pos + 1..].to_string())
        } else {
            (value.to_string(), String::new())
        }
    }

    /// Get first audio media description
    pub fn audio_media(&self) -> Option<&SdpMedia> {
        self.media.iter().find(|m| m.media_type == "audio")
    }
}
```

---

### 38.2 RAOP-Specific Parameter Extraction

- [x] **38.2.1** Extract ALAC/AAC format parameters

**File:** `src/protocol/sdp/raop.rs`

```rust
//! RAOP-specific SDP parsing
//!
//! Extracts audio format parameters from RAOP ANNOUNCE SDP.

use super::parser::{SdpSession, SdpMedia, SdpParseError};
use crate::receiver::session::{StreamParameters, AudioCodec};
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
    pub fn parse(fmtp: &str) -> Result<Self, SdpParseError> {
        let parts: Vec<&str> = fmtp.split_whitespace().collect();

        // Skip payload type (first element)
        if parts.len() < 12 {
            return Err(SdpParseError::InvalidFmtp(format!(
                "ALAC fmtp needs 12 fields, got {}: {}",
                parts.len(),
                fmtp
            )));
        }

        // Start from index 1 (skip payload type)
        let offset = if parts[0].parse::<u32>().is_ok() { 1 } else { 0 };

        Ok(AlacParameters {
            frames_per_packet: parts.get(offset).and_then(|s| s.parse().ok()).unwrap_or(352),
            compatible_version: parts.get(offset + 1).and_then(|s| s.parse().ok()).unwrap_or(0),
            bit_depth: parts.get(offset + 2).and_then(|s| s.parse().ok()).unwrap_or(16),
            pb: parts.get(offset + 3).and_then(|s| s.parse().ok()).unwrap_or(40),
            mb: parts.get(offset + 4).and_then(|s| s.parse().ok()).unwrap_or(10),
            kb: parts.get(offset + 5).and_then(|s| s.parse().ok()).unwrap_or(14),
            channels: parts.get(offset + 6).and_then(|s| s.parse().ok()).unwrap_or(2),
            max_run: parts.get(offset + 7).and_then(|s| s.parse().ok()).unwrap_or(255),
            max_frame_bytes: parts.get(offset + 8).and_then(|s| s.parse().ok()).unwrap_or(0),
            avg_bit_rate: parts.get(offset + 9).and_then(|s| s.parse().ok()).unwrap_or(0),
            sample_rate: parts.get(offset + 10).and_then(|s| s.parse().ok()).unwrap_or(44100),
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
pub fn parse_encryption(media: &SdpMedia) -> Result<Option<EncryptionParams>, SdpParseError> {
    let encrypted_key = match media.attributes.get("rsaaeskey") {
        Some(key) => key,
        None => return Ok(None),  // No encryption
    };

    let iv_str = media.attributes.get("aesiv")
        .ok_or_else(|| SdpParseError::MissingField("aesiv".to_string()))?;

    // Decode base64
    let encrypted_aes_key = BASE64.decode(encrypted_key.trim())
        .map_err(|_| SdpParseError::InvalidLine("Invalid base64 in rsaaeskey".to_string()))?;

    let iv_bytes = BASE64.decode(iv_str.trim())
        .map_err(|_| SdpParseError::InvalidLine("Invalid base64 in aesiv".to_string()))?;

    if iv_bytes.len() != 16 {
        return Err(SdpParseError::InvalidLine(
            format!("AES IV must be 16 bytes, got {}", iv_bytes.len())
        ));
    }

    let mut aes_iv = [0u8; 16];
    aes_iv.copy_from_slice(&iv_bytes);

    Ok(Some(EncryptionParams {
        encrypted_aes_key,
        aes_iv,
    }))
}

/// Detect codec from rtpmap attribute
pub fn detect_codec(media: &SdpMedia) -> Option<AudioCodec> {
    let rtpmap = media.attributes.get("rtpmap")?;

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
pub fn extract_stream_parameters(
    sdp: &SdpSession,
    rsa_private_key: Option<&[u8]>,
) -> Result<StreamParameters, SdpParseError> {
    let media = sdp.audio_media()
        .ok_or_else(|| SdpParseError::MissingField("audio media".to_string()))?;

    let codec = detect_codec(media)
        .ok_or_else(|| SdpParseError::MissingField("rtpmap".to_string()))?;

    let (sample_rate, bits_per_sample, channels, frames_per_packet) = match codec {
        AudioCodec::Alac => {
            let fmtp = media.attributes.get("fmtp")
                .ok_or_else(|| SdpParseError::MissingField("fmtp".to_string()))?;
            let alac = AlacParameters::parse(fmtp)?;
            (alac.sample_rate, alac.bit_depth, alac.channels, alac.frames_per_packet)
        }
        AudioCodec::Pcm => {
            // L16 defaults
            (44100, 16, 2, 352)
        }
        AudioCodec::AacLc | AudioCodec::AacEld => {
            // AAC defaults
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
    let min_latency = media.attributes.get("min-latency")
        .and_then(|s| s.parse().ok());

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
fn decrypt_aes_key(
    encrypted: &[u8],
    rsa_private_key: &[u8],
) -> Result<[u8; 16], SdpParseError> {
    use rsa::{RsaPrivateKey, Pkcs1v15Encrypt};
    use rsa::pkcs8::DecodePrivateKey;

    // Parse RSA private key
    let private_key = RsaPrivateKey::from_pkcs8_der(rsa_private_key)
        .map_err(|e| SdpParseError::InvalidLine(format!("Invalid RSA key: {}", e)))?;

    // Decrypt using PKCS#1 v1.5
    let decrypted = private_key.decrypt(Pkcs1v15Encrypt, encrypted)
        .map_err(|e| SdpParseError::InvalidLine(format!("RSA decrypt failed: {}", e)))?;

    if decrypted.len() != 16 {
        return Err(SdpParseError::InvalidLine(
            format!("Decrypted AES key must be 16 bytes, got {}", decrypted.len())
        ));
    }

    let mut key = [0u8; 16];
    key.copy_from_slice(&decrypted);
    Ok(key)
}
```

---

### 38.3 Integration with Session

- [x] **38.3.1** Wire SDP parsing into ANNOUNCE handler

**File:** `src/receiver/announce_handler.rs`

```rust
//! ANNOUNCE request handler
//!
//! Processes ANNOUNCE requests and configures session stream parameters.

use crate::protocol::sdp::{parser::SdpSession, raop::extract_stream_parameters};
use crate::receiver::session::{ReceiverSession, StreamParameters};
use crate::protocol::rtsp::RtspRequest;

/// Errors from ANNOUNCE handling
#[derive(Debug, thiserror::Error)]
pub enum AnnounceError {
    #[error("Empty body in ANNOUNCE")]
    EmptyBody,

    #[error("Body is not valid UTF-8")]
    InvalidUtf8,

    #[error("SDP parse error: {0}")]
    SdpParse(#[from] crate::protocol::sdp::parser::SdpParseError),

    #[error("Unsupported codec")]
    UnsupportedCodec,
}

/// Process an ANNOUNCE request
pub fn process_announce(
    request: &RtspRequest,
    rsa_private_key: Option<&[u8]>,
) -> Result<StreamParameters, AnnounceError> {
    if request.body.is_empty() {
        return Err(AnnounceError::EmptyBody);
    }

    let sdp_str = std::str::from_utf8(&request.body)
        .map_err(|_| AnnounceError::InvalidUtf8)?;

    let sdp = SdpSession::parse(sdp_str)?;

    let params = extract_stream_parameters(&sdp, rsa_private_key)?;

    Ok(params)
}

/// Apply stream parameters to session
pub fn apply_to_session(
    session: &mut ReceiverSession,
    params: StreamParameters,
) {
    session.set_stream_params(params);
}
```

---

## Unit Tests

### 38.4 Unit Tests

- [x] **38.4.1** SDP parsing tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SDP: &str = r#"v=0
o=iTunes 3413821438 0 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.1
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:VGhpcyBpcyBhIHRlc3Qga2V5IHRoYXQgaXMgdXNlZCBmb3IgdGVzdGluZw==
a=aesiv:MDEyMzQ1Njc4OWFiY2RlZg==
a=min-latency:11025
"#;

    const SIMPLE_SDP: &str = r#"v=0
o=- 0 0 IN IP4 127.0.0.1
s=AirTunes
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
"#;

    #[test]
    fn test_parse_basic_sdp() {
        let sdp = SdpSession::parse(SIMPLE_SDP).unwrap();

        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.session_name, "AirTunes");
        assert_eq!(sdp.media.len(), 1);

        let audio = sdp.audio_media().unwrap();
        assert_eq!(audio.media_type, "audio");
        assert!(audio.attributes.contains_key("rtpmap"));
    }

    #[test]
    fn test_parse_full_sdp() {
        let sdp = SdpSession::parse(SAMPLE_SDP).unwrap();

        assert!(sdp.origin.is_some());
        let origin = sdp.origin.as_ref().unwrap();
        assert_eq!(origin.username, "iTunes");

        let audio = sdp.audio_media().unwrap();
        assert!(audio.attributes.contains_key("rsaaeskey"));
        assert!(audio.attributes.contains_key("aesiv"));
        assert!(audio.attributes.contains_key("min-latency"));
    }

    #[test]
    fn test_detect_codec_alac() {
        let sdp = SdpSession::parse(SIMPLE_SDP).unwrap();
        let audio = sdp.audio_media().unwrap();

        let codec = detect_codec(audio).unwrap();
        assert_eq!(codec, AudioCodec::Alac);
    }

    #[test]
    fn test_parse_alac_parameters() {
        let fmtp = "96 352 0 16 40 10 14 2 255 0 0 44100";
        let params = AlacParameters::parse(fmtp).unwrap();

        assert_eq!(params.frames_per_packet, 352);
        assert_eq!(params.bit_depth, 16);
        assert_eq!(params.channels, 2);
        assert_eq!(params.sample_rate, 44100);
    }

    #[test]
    fn test_parse_encryption_params() {
        let sdp = SdpSession::parse(SAMPLE_SDP).unwrap();
        let audio = sdp.audio_media().unwrap();

        let enc = parse_encryption(audio).unwrap();
        assert!(enc.is_some());

        let enc = enc.unwrap();
        assert!(!enc.encrypted_aes_key.is_empty());
        assert_eq!(enc.aes_iv.len(), 16);
    }

    #[test]
    fn test_no_encryption() {
        let sdp = SdpSession::parse(SIMPLE_SDP).unwrap();
        let audio = sdp.audio_media().unwrap();

        let enc = parse_encryption(audio).unwrap();
        assert!(enc.is_none());
    }

    #[test]
    fn test_extract_stream_params_unencrypted() {
        let sdp = SdpSession::parse(SIMPLE_SDP).unwrap();

        let params = extract_stream_parameters(&sdp, None).unwrap();

        assert_eq!(params.codec, AudioCodec::Alac);
        assert_eq!(params.sample_rate, 44100);
        assert_eq!(params.bits_per_sample, 16);
        assert_eq!(params.channels, 2);
        assert_eq!(params.frames_per_packet, 352);
        assert!(params.aes_key.is_none());
    }

    #[test]
    fn test_pcm_codec() {
        let sdp_str = r#"v=0
o=- 0 0 IN IP4 127.0.0.1
s=Test
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 L16/44100/2
"#;
        let sdp = SdpSession::parse(sdp_str).unwrap();
        let audio = sdp.audio_media().unwrap();

        let codec = detect_codec(audio).unwrap();
        assert_eq!(codec, AudioCodec::Pcm);
    }

    #[test]
    fn test_min_latency_extraction() {
        let sdp = SdpSession::parse(SAMPLE_SDP).unwrap();
        let params = extract_stream_parameters(&sdp, None).unwrap();

        assert_eq!(params.min_latency, Some(11025));
    }
}
```

---

## Acceptance Criteria

- [x] Parse SDP v0 sessions correctly
- [x] Extract audio media section
- [x] Parse rtpmap for codec detection (ALAC, AAC, PCM)
- [x] Parse fmtp for ALAC parameters
- [x] Decode base64 rsaaeskey and aesiv
- [x] Decrypt AES key with RSA private key (when provided)
- [x] Handle missing encryption (unencrypted streams)
- [x] Extract min-latency attribute
- [x] Map to StreamParameters correctly
- [x] All unit tests pass

---

## Notes

- **RSA Key**: The receiver needs a compatible RSA private key; for testing, a self-generated key works
- **Apple's Key**: Real AirPlay devices use Apple's private key; open-source implementations use extracted keys
- **Codec Support**: Focus on ALAC first (most common), then PCM, then AAC
- **Robustness**: Real-world SDPs may have variations; be permissive in parsing

---

## References

- [RFC 4566](https://tools.ietf.org/html/rfc4566) - SDP: Session Description Protocol
- [RAOP SDP Format](https://nto.github.io/AirPlay.html#audio-sdp)
- [ALAC Specification](https://macosforge.github.io/alac/)
