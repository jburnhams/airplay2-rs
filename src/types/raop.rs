use std::collections::HashMap;

/// Supported audio codecs for RAOP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopCodec {
    /// Uncompressed PCM
    Pcm = 0,
    /// Apple Lossless Audio Codec
    Alac = 1,
    /// Advanced Audio Coding
    Aac = 2,
    /// AAC Enhanced Low Delay (for screen mirroring)
    AacEld = 3,
}

impl RaopCodec {
    /// Parse from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Pcm),
            1 => Some(Self::Alac),
            2 => Some(Self::Aac),
            3 => Some(Self::AacEld),
            _ => None,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pcm => "PCM",
            Self::Alac => "Apple Lossless",
            Self::Aac => "AAC",
            Self::AacEld => "AAC-ELD",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_capabilities_basic() {
        let mut records = HashMap::new();
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2".to_string());
        records.insert("et".to_string(), "0,1".to_string());
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert_eq!(caps.sample_size, 16);
        assert!(caps.supports_codec(RaopCodec::Pcm));
        assert!(caps.supports_codec(RaopCodec::Alac));
        assert!(caps.supports_codec(RaopCodec::Aac));
        assert!(caps.supports_rsa());
        assert!(caps.supports_unencrypted());
    }

    #[test]
    fn test_parse_capabilities_airport_express() {
        // Typical AirPort Express TXT records
        let mut records = HashMap::new();
        records.insert("txtvers".to_string(), "1".to_string());
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2,3".to_string());
        records.insert("da".to_string(), "true".to_string());
        records.insert("et".to_string(), "0,3,5".to_string());
        records.insert("md".to_string(), "0,1,2".to_string());
        records.insert("pw".to_string(), "false".to_string());
        records.insert("sr".to_string(), "44100".to_string());
        records.insert("ss".to_string(), "16".to_string());
        records.insert("tp".to_string(), "UDP".to_string());
        records.insert("vs".to_string(), "130.14".to_string());
        records.insert("am".to_string(), "AirPort10,115".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert!(caps.metadata_support);
        assert!(!caps.password_required);
        assert_eq!(caps.model, Some("AirPort10,115".to_string()));
        assert_eq!(caps.preferred_codec(), Some(RaopCodec::Alac));
    }

    #[test]
    fn test_preferred_encryption_rsa() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "0,1".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::Rsa));
    }

    #[test]
    fn test_preferred_encryption_none_only() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "0".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::None));
    }

    #[test]
    fn test_preferred_encryption_fairplay_unsupported() {
        let mut records = HashMap::new();
        records.insert("et".to_string(), "3".to_string()); // FairPlay only

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.preferred_encryption(), None);
    }

    #[test]
    fn test_codec_preference() {
        let mut records = HashMap::new();
        records.insert("cn".to_string(), "0,2".to_string()); // PCM and AAC

        let caps = RaopCapabilities::from_txt_records(&records);

        // Should prefer AAC over PCM
        assert_eq!(caps.preferred_codec(), Some(RaopCodec::Aac));
    }

    #[test]
    fn test_empty_records() {
        let records = HashMap::new();
        let caps = RaopCapabilities::from_txt_records(&records);

        // Should use sensible defaults
        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert_eq!(caps.sample_size, 16);
        assert_eq!(caps.transport, "UDP");
    }
}

/// Supported encryption types for RAOP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopEncryption {
    /// No encryption
    None = 0,
    /// RSA (AirPort Express original)
    Rsa = 1,
    /// FairPlay (iTunes DRM)
    FairPlay = 3,
    /// MFi-SAP (third-party devices)
    MfiSap = 4,
    /// FairPlay SAPv2.5 (iOS/macOS mirroring)
    FairPlaySap25 = 5,
}

impl RaopEncryption {
    /// Parse from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Rsa),
            3 => Some(Self::FairPlay),
            4 => Some(Self::MfiSap),
            5 => Some(Self::FairPlaySap25),
            _ => None,
        }
    }

    /// Check if this encryption type is supported by the library
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::None | Self::Rsa)
    }
}

/// Metadata types supported by RAOP devices
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopMetadataType {
    /// Text metadata (track, artist, album)
    Text = 0,
    /// Artwork images
    Artwork = 1,
    /// Playback progress
    Progress = 2,
}

impl RaopMetadataType {
    /// Parse from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Text),
            1 => Some(Self::Artwork),
            2 => Some(Self::Progress),
            _ => None,
        }
    }
}

/// RAOP device capabilities parsed from TXT records
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RaopCapabilities {
    /// TXT record version
    pub txt_version: u8,
    /// Number of audio channels
    pub channels: u8,
    /// Supported codecs
    pub codecs: Vec<RaopCodec>,
    /// Supported encryption types
    pub encryption_types: Vec<RaopEncryption>,
    /// Supports metadata
    pub metadata_support: bool,
    /// Supported metadata types
    pub metadata_types: Vec<RaopMetadataType>,
    /// Password required
    pub password_required: bool,
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Sample size (bits)
    pub sample_size: u8,
    /// Transport protocol
    pub transport: String,
    /// Server version string
    pub server_version: Option<String>,
    /// Device model
    pub model: Option<String>,
    /// Status flags
    pub status_flags: u32,
}

/// RAOP TXT record keys
pub mod txt_keys {
    /// TXT record version (usually "1")
    pub const TXTVERS: &str = "txtvers";
    /// Number of audio channels (e.g., "2" for stereo)
    pub const CHANNELS: &str = "ch";
    /// Supported codecs (e.g., "0,1,2,3")
    pub const CODECS: &str = "cn";
    /// Metadata support flag
    pub const METADATA: &str = "da";
    /// Supported encryption types (e.g., "0,1,3,5")
    pub const ENCRYPTION: &str = "et";
    /// Supported metadata types (e.g., "0,1,2")
    pub const METADATA_TYPES: &str = "md";
    /// Password required flag
    pub const PASSWORD: &str = "pw";
    /// Sample rate in Hz (e.g., "44100")
    pub const SAMPLE_RATE: &str = "sr";
    /// Sample size in bits (e.g., "16")
    pub const SAMPLE_SIZE: &str = "ss";
    /// Transport protocol (e.g., "UDP")
    pub const TRANSPORT: &str = "tp";
    /// Server version (e.g., "130.14")
    pub const VERSION: &str = "vs";
    /// Version number (e.g., "65537")
    pub const VERSION_NUM: &str = "vn";
    /// Device model (e.g., "AppleTV2,1")
    pub const MODEL: &str = "am";
    /// Status flags
    pub const FLAGS: &str = "sf";
}

impl RaopCapabilities {
    /// Parse from TXT record map
    pub fn from_txt_records(records: &HashMap<String, String>) -> Self {
        let mut caps = Self::default();

        // Parse txtvers
        if let Some(v) = records.get(txt_keys::TXTVERS) {
            caps.txt_version = v.parse().unwrap_or(1);
        }

        // Parse channels
        if let Some(v) = records.get(txt_keys::CHANNELS) {
            caps.channels = v.parse().unwrap_or(2);
        } else {
            caps.channels = 2; // Default stereo
        }

        // Parse codecs (comma-separated list)
        if let Some(v) = records.get(txt_keys::CODECS) {
            caps.codecs = Self::parse_codec_list(v);
        }

        // Parse encryption types
        if let Some(v) = records.get(txt_keys::ENCRYPTION) {
            caps.encryption_types = Self::parse_encryption_list(v);
        }

        // Parse metadata support
        if let Some(v) = records.get(txt_keys::METADATA) {
            caps.metadata_support = v == "true" || v == "1";
        }

        // Parse metadata types
        if let Some(v) = records.get(txt_keys::METADATA_TYPES) {
            caps.metadata_types = Self::parse_metadata_types(v);
        }

        // Parse password requirement
        if let Some(v) = records.get(txt_keys::PASSWORD) {
            caps.password_required = v == "true" || v == "1";
        }

        // Parse sample rate
        if let Some(v) = records.get(txt_keys::SAMPLE_RATE) {
            caps.sample_rate = v.parse().unwrap_or(44100);
        } else {
            caps.sample_rate = 44100;
        }

        // Parse sample size
        if let Some(v) = records.get(txt_keys::SAMPLE_SIZE) {
            caps.sample_size = v.parse().unwrap_or(16);
        } else {
            caps.sample_size = 16;
        }

        // Parse transport
        if let Some(v) = records.get(txt_keys::TRANSPORT) {
            caps.transport = v.clone();
        } else {
            caps.transport = "UDP".to_string();
        }

        // Optional fields
        caps.server_version = records.get(txt_keys::VERSION).cloned();
        caps.model = records.get(txt_keys::MODEL).cloned();

        if let Some(v) = records.get(txt_keys::FLAGS) {
            caps.status_flags = u32::from_str_radix(v.trim_start_matches("0x"), 16)
                .unwrap_or(0);
        }

        caps
    }

    fn parse_codec_list(s: &str) -> Vec<RaopCodec> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopCodec::from_u8)
            .collect()
    }

    fn parse_encryption_list(s: &str) -> Vec<RaopEncryption> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopEncryption::from_u8)
            .collect()
    }

    fn parse_metadata_types(s: &str) -> Vec<RaopMetadataType> {
        s.split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .filter_map(RaopMetadataType::from_u8)
            .collect()
    }

    /// Check if device supports a specific codec
    pub fn supports_codec(&self, codec: RaopCodec) -> bool {
        self.codecs.contains(&codec)
    }

    /// Check if device supports RSA encryption
    pub fn supports_rsa(&self) -> bool {
        self.encryption_types.contains(&RaopEncryption::Rsa)
    }

    /// Check if device supports unencrypted streaming
    pub fn supports_unencrypted(&self) -> bool {
        self.encryption_types.contains(&RaopEncryption::None)
    }

    /// Get preferred codec (ALAC > AAC > PCM)
    pub fn preferred_codec(&self) -> Option<RaopCodec> {
        if self.codecs.contains(&RaopCodec::Alac) {
            Some(RaopCodec::Alac)
        } else if self.codecs.contains(&RaopCodec::Aac) {
            Some(RaopCodec::Aac)
        } else if self.codecs.contains(&RaopCodec::Pcm) {
            Some(RaopCodec::Pcm)
        } else {
            self.codecs.first().copied()
        }
    }

    /// Get preferred encryption (RSA if available, else None)
    pub fn preferred_encryption(&self) -> Option<RaopEncryption> {
        if self.supports_rsa() {
            Some(RaopEncryption::Rsa)
        } else if self.supports_unencrypted() {
            Some(RaopEncryption::None)
        } else {
            None
        }
    }
}
