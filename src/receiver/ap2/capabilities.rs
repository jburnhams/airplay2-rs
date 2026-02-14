//! Device capabilities for `AirPlay` 2 receiver
//!
//! These structures define what our receiver advertises to senders
//! via the /info endpoint.

use crate::protocol::plist::PlistValue;
use std::collections::HashMap;

/// Device capabilities for /info response
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeviceCapabilities {
    /// Device identification
    pub device_id: String,
    /// Device name
    pub name: String,
    /// Device model
    pub model: String,
    /// Device manufacturer
    pub manufacturer: String,
    /// Device serial number
    pub serial_number: Option<String>,

    /// Version information
    pub source_version: String,
    /// Protocol version
    pub protocol_version: String,
    /// Firmware version
    pub firmware_version: String,
    /// OS build version
    pub os_build_version: Option<String>,

    /// Feature flags (64-bit)
    pub features: u64,

    /// Status flags (32-bit)
    pub status_flags: u32,

    /// Public key for pairing (Ed25519)
    pub public_key: [u8; 32],

    /// Pairing identity (UUID)
    pub pairing_identity: String,

    /// Audio capabilities
    pub audio_formats: Vec<AudioFormatCapability>,
    /// Audio latencies configuration
    pub audio_latencies: AudioLatencies,

    /// Display capabilities (for screen mirroring, optional)
    pub displays: Vec<DisplayCapability>,

    /// Volume control: initial volume
    pub initial_volume: f32,
    /// Volume control: whether volume control is supported
    pub supports_volume: bool,

    /// Group/multi-room: Group UUID
    pub group_uuid: Option<String>,
    /// Group/multi-room: Is group leader
    pub group_leader: bool,

    /// Timing: Supports PTP
    pub supports_ptp: bool,
    /// Timing: PTP Clock ID
    pub ptp_clock_id: Option<u64>,

    /// Authentication requirements: Requires password
    pub requires_password: bool,
    /// Authentication requirements: Supports `HomeKit`
    pub supports_homekit: bool,
}

/// Audio format capability
#[derive(Debug, Clone)]
pub struct AudioFormatCapability {
    /// Format type (96 = ALAC, 97 = AAC-ELD, etc.)
    pub type_id: u32,
    /// Audio channels
    pub channels: u8,
    /// Sample rates (Hz)
    pub sample_rates: Vec<u32>,
    /// Bits per sample
    pub bits_per_sample: Vec<u8>,
    /// Encryption types supported
    pub encryption_types: Vec<EncryptionType>,
}

/// Encryption types for audio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionType {
    /// No encryption
    None = 0,
    /// RSA (`AirPlay` 1)
    Rsa = 1,
    /// `FairPlay`
    FairPlay = 2,
    /// `MFi` SAP
    MfiSap = 3,
    /// `FairPlay` SAP
    FairPlaySap = 4,
}

/// Audio latency configuration
#[derive(Debug, Clone)]
pub struct AudioLatencies {
    /// Minimum supported latency (ms)
    pub min_latency_ms: u32,
    /// Maximum supported latency (ms)
    pub max_latency_ms: u32,
    /// Default latency (ms)
    pub default_latency_ms: u32,
    /// Latency for buffered (multi-room) mode
    pub buffered_latency_ms: u32,
}

/// Display capability (for screen mirroring)
#[derive(Debug, Clone)]
pub struct DisplayCapability {
    /// Display width
    pub width: u32,
    /// Display height
    pub height: u32,
    /// Display refresh rate
    pub refresh_rate: f32,
    /// Display UUID
    pub uuid: String,
    /// Display features
    pub features: u32,
}

impl Default for AudioLatencies {
    fn default() -> Self {
        Self {
            min_latency_ms: 0,
            max_latency_ms: 4000,
            default_latency_ms: 2000,
            buffered_latency_ms: 2000,
        }
    }
}

impl DeviceCapabilities {
    /// Create default capabilities for an audio-only receiver
    #[must_use]
    pub fn audio_receiver(device_id: &str, name: &str, public_key: [u8; 32]) -> Self {
        Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            model: "Receiver1,1".to_string(),
            manufacturer: "airplay2-rs".to_string(),
            serial_number: None,

            source_version: "366.0".to_string(),
            protocol_version: "1.1".to_string(),
            firmware_version: env!("CARGO_PKG_VERSION").to_string(),
            os_build_version: None,

            features: Self::default_audio_features(),
            status_flags: 0x04, // Supports PIN

            public_key,
            pairing_identity: Self::derive_pairing_identity(device_id),

            audio_formats: Self::default_audio_formats(),
            audio_latencies: AudioLatencies::default(),

            displays: vec![],

            initial_volume: 0.5,
            supports_volume: true,

            group_uuid: None,
            group_leader: false,

            supports_ptp: true,
            ptp_clock_id: None,

            requires_password: false,
            supports_homekit: true,
        }
    }

    /// Default feature flags for audio receiver
    fn default_audio_features() -> u64 {
        let mut features: u64 = 0;

        // Core audio
        features |= 1 << 9; // Audio
        features |= 1 << 11; // Audio redundant

        // Audio formats
        features |= 1 << 22; // ALAC
        features |= 1 << 23; // AAC-LC
        features |= 1 << 25; // AAC-ELD

        // Authentication
        features |= 1 << 14; // Auth setup
        features |= 1 << 17; // Legacy pairing
        features |= 1 << 26; // PIN
        features |= 1 << 27; // Transient pairing
        features |= 1 << 46; // HomeKit

        // Timing
        features |= 1 << 38; // Buffered audio
        features |= 1 << 40; // PTP

        // Control
        features |= 1 << 18; // Unified media control
        features |= 1 << 19; // Volume control

        features
    }

    /// Default audio formats for receiver
    fn default_audio_formats() -> Vec<AudioFormatCapability> {
        vec![
            // ALAC
            AudioFormatCapability {
                type_id: 96,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16, 24],
                encryption_types: vec![EncryptionType::None],
            },
            // AAC-LC
            AudioFormatCapability {
                type_id: 97,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16],
                encryption_types: vec![EncryptionType::None],
            },
            // AAC-ELD
            AudioFormatCapability {
                type_id: 98,
                channels: 2,
                sample_rates: vec![44100, 48000],
                bits_per_sample: vec![16],
                encryption_types: vec![EncryptionType::None],
            },
            // PCM
            AudioFormatCapability {
                type_id: 100,
                channels: 2,
                sample_rates: vec![44100, 48000, 96000],
                bits_per_sample: vec![16, 24],
                encryption_types: vec![EncryptionType::None],
            },
        ]
    }

    /// Derive pairing identity from device ID
    fn derive_pairing_identity(device_id: &str) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update(b"AirPlay2-PI");
        let hash = hasher.finalize();

        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            hash[0],
            hash[1],
            hash[2],
            hash[3],
            hash[4],
            hash[5],
            (hash[6] & 0x0f) | 0x40,
            hash[7],
            (hash[8] & 0x3f) | 0x80,
            hash[9],
            hash[10],
            hash[11],
            hash[12],
            hash[13],
            hash[14],
            hash[15]
        )
    }

    /// Convert to binary plist value
    #[must_use]
    pub fn to_plist(&self) -> PlistValue {
        let mut dict: HashMap<String, PlistValue> = HashMap::new();

        // Device identification
        dict.insert(
            "deviceid".to_string(),
            PlistValue::String(self.device_id.clone()),
        );
        dict.insert("name".to_string(), PlistValue::String(self.name.clone()));
        dict.insert("model".to_string(), PlistValue::String(self.model.clone()));
        dict.insert(
            "manufacturer".to_string(),
            PlistValue::String(self.manufacturer.clone()),
        );

        if let Some(ref serial) = self.serial_number {
            dict.insert(
                "serialNumber".to_string(),
                PlistValue::String(serial.clone()),
            );
        }

        // Version information
        dict.insert(
            "srcvers".to_string(),
            PlistValue::String(self.source_version.clone()),
        );
        dict.insert(
            "protovers".to_string(),
            PlistValue::String(self.protocol_version.clone()),
        );
        dict.insert(
            "fv".to_string(),
            PlistValue::String(self.firmware_version.clone()),
        );

        if let Some(ref os_build) = self.os_build_version {
            dict.insert(
                "osBuildVersion".to_string(),
                PlistValue::String(os_build.clone()),
            );
        }

        // Feature flags
        dict.insert("features".to_string(), PlistValue::from(self.features));
        dict.insert(
            "statusFlags".to_string(),
            PlistValue::Integer(i64::from(self.status_flags)),
        );

        // Public key
        dict.insert("pk".to_string(), PlistValue::Data(self.public_key.to_vec()));

        // Pairing identity
        dict.insert(
            "pi".to_string(),
            PlistValue::String(self.pairing_identity.clone()),
        );

        // Audio formats
        dict.insert("audioFormats".to_string(), self.audio_formats_to_plist());

        // Audio latencies
        dict.insert(
            "audioLatencies".to_string(),
            self.audio_latencies_to_plist(),
        );

        // Volume
        dict.insert(
            "initialVolume".to_string(),
            PlistValue::Real(f64::from(self.initial_volume)),
        );

        // Timing
        if self.supports_ptp {
            dict.insert("supportsPTP".to_string(), PlistValue::Boolean(true));
            if let Some(clock_id) = self.ptp_clock_id {
                dict.insert("ptpClockID".to_string(), PlistValue::from(clock_id));
            }
        }

        // Group
        if let Some(ref group_uuid) = self.group_uuid {
            dict.insert(
                "groupUUID".to_string(),
                PlistValue::String(group_uuid.clone()),
            );
        }
        dict.insert(
            "isGroupLeader".to_string(),
            PlistValue::Boolean(self.group_leader),
        );

        PlistValue::Dictionary(dict)
    }

    fn audio_formats_to_plist(&self) -> PlistValue {
        let formats: Vec<PlistValue> = self
            .audio_formats
            .iter()
            .map(|fmt| {
                let mut dict: HashMap<String, PlistValue> = HashMap::new();
                dict.insert(
                    "type".to_string(),
                    PlistValue::Integer(i64::from(fmt.type_id)),
                );
                dict.insert(
                    "ch".to_string(),
                    PlistValue::Integer(i64::from(fmt.channels)),
                );

                let sample_rates: Vec<PlistValue> = fmt
                    .sample_rates
                    .iter()
                    .map(|&sr| PlistValue::Integer(i64::from(sr)))
                    .collect();
                dict.insert("sr".to_string(), PlistValue::Array(sample_rates));

                let bits: Vec<PlistValue> = fmt
                    .bits_per_sample
                    .iter()
                    .map(|&b| PlistValue::Integer(i64::from(b)))
                    .collect();
                dict.insert("ss".to_string(), PlistValue::Array(bits));

                let enc_types: Vec<PlistValue> = fmt
                    .encryption_types
                    .iter()
                    .map(|&e| PlistValue::Integer(e as i64))
                    .collect();
                dict.insert("et".to_string(), PlistValue::Array(enc_types));

                PlistValue::Dictionary(dict)
            })
            .collect();

        PlistValue::Array(formats)
    }

    fn audio_latencies_to_plist(&self) -> PlistValue {
        let mut latency_entry: HashMap<String, PlistValue> = HashMap::new();
        latency_entry.insert("inputLatencyMicros".to_string(), PlistValue::Integer(0));
        latency_entry.insert(
            "outputLatencyMicros".to_string(),
            PlistValue::Integer(i64::from(self.audio_latencies.default_latency_ms * 1000)),
        );
        latency_entry.insert("type".to_string(), PlistValue::Integer(96)); // Default format
        latency_entry.insert(
            "audioType".to_string(),
            PlistValue::String("default".to_string()),
        );

        PlistValue::Array(vec![PlistValue::Dictionary(latency_entry)])
    }
}
