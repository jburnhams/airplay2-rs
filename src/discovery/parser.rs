//! Parser for `AirPlay` TXT record data

use crate::types::DeviceCapabilities;
use std::collections::HashMap;

/// Parse TXT records from mDNS response
#[must_use]
pub fn parse_txt_records(records: &[String]) -> HashMap<String, String> {
    records
        .iter()
        .filter_map(|record| {
            let mut parts = record.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((key, value))
        })
        .collect()
}

/// Parse features flags from TXT record
///
/// The features value can be in hex format: "0x1234567890ABCDEF"
/// or comma-separated: "0x1234,0x5678"
#[must_use]
pub fn parse_features(features_str: &str) -> Option<DeviceCapabilities> {
    let features = if features_str.contains(',') {
        // Comma-separated format: "low,high" (e.g. "0x1234,0x5678")
        // Combine into single 64-bit value
        let parts: Vec<&str> = features_str.split(',').collect();
        if parts.len() >= 2 {
            let lo = parse_hex(parts[0])?;
            let hi = parse_hex(parts[1])?;
            (hi << 32) | lo
        } else {
            parse_hex(parts[0])?
        }
    } else {
        // Single hex value
        parse_hex(features_str)?
    };

    Some(DeviceCapabilities::from_features(features))
}

/// Parse hex string to u64
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Parse device model from model string
#[must_use]
pub fn parse_model_name(model: &str) -> &str {
    // Map internal model identifiers to friendly names
    match model {
        "AudioAccessory1,1" | "AudioAccessory1,2" => "HomePod",
        "AudioAccessory5,1" => "HomePod mini",
        "AppleTV3,1" | "AppleTV3,2" => "Apple TV (3rd generation)",
        "AppleTV5,3" => "Apple TV (4th generation)",
        "AppleTV6,2" => "Apple TV 4K",
        "AppleTV11,1" => "Apple TV 4K (2nd generation)",
        "AppleTV14,1" => "Apple TV 4K (3rd generation)",
        "AirPort10,1" => "AirPort Express",
        _ => model,
    }
}

/// Known TXT record keys for `AirPlay`
pub mod txt_keys {
    /// Device ID (MAC address format)
    pub const DEVICE_ID: &str = "deviceid";
    /// Features bitmask
    pub const FEATURES: &str = "features";
    /// Flags
    pub const FLAGS: &str = "flags";
    /// Model identifier
    pub const MODEL: &str = "model";
    /// Protocol version
    pub const PROTOCOL_VERSION: &str = "protovers";
    /// Source version
    pub const SOURCE_VERSION: &str = "srcvers";
    /// Volume (0-1)
    pub const VOLUME: &str = "vv";
    /// Public key (for pairing)
    pub const PUBLIC_KEY: &str = "pk";
    /// Password required
    pub const PASSWORD: &str = "pw";
    /// PIN required
    pub const PIN: &str = "pin";
    /// Group contains discoverable leader
    pub const GROUP_CONTAINS_LEADER: &str = "gcgl";
    /// Group UUID
    pub const GROUP_UUID: &str = "gid";
    /// Is group leader
    pub const IS_GROUP_LEADER: &str = "igl";
    /// `AirPlay` version
    pub const AIRPLAY_VERSION: &str = "am";
}

/// `AirPlay` feature bits
///
/// Reference: <https://emanuelecozzi.net/docs/airplay2/features>
pub mod feature_bits {
    /// Video supported
    pub const VIDEO: u64 = 1 << 0;
    /// Photo supported
    pub const PHOTO: u64 = 1 << 1;
    /// Video fade in
    pub const VIDEO_FADE_IN: u64 = 1 << 2;
    /// Video HTTP live streaming
    pub const VIDEO_HLS: u64 = 1 << 3;
    /// Slideshow supported
    pub const SLIDESHOW: u64 = 1 << 5;
    /// Screen mirroring
    pub const SCREEN: u64 = 1 << 7;
    /// Screen rotation
    pub const SCREEN_ROTATE: u64 = 1 << 8;
    /// Audio supported
    pub const AUDIO: u64 = 1 << 9;
    /// Audio redundant
    pub const AUDIO_REDUNDANT: u64 = 1 << 11;
    /// `FairPlay` secure auth
    pub const FPS_AP_V2_5: u64 = 1 << 12;
    /// Photo caching
    pub const PHOTO_CACHING: u64 = 1 << 13;
    /// Authentication type 4
    pub const AUTH_TYPE_4: u64 = 1 << 14;
    /// Metadata type 1
    pub const METADATA_TYPE_1: u64 = 1 << 15;
    /// Metadata type 2
    pub const METADATA_TYPE_2: u64 = 1 << 16;
    /// Metadata type 3 (artwork)
    pub const METADATA_TYPE_3: u64 = 1 << 17;
    /// Audio format 1
    pub const AUDIO_FORMAT_1: u64 = 1 << 18;
    /// Audio format 2
    pub const AUDIO_FORMAT_2: u64 = 1 << 19;
    /// Audio format 3
    pub const AUDIO_FORMAT_3: u64 = 1 << 20;
    /// Audio format 4
    pub const AUDIO_FORMAT_4: u64 = 1 << 21;
    /// Authentication type 1
    pub const AUTH_TYPE_1: u64 = 1 << 23;
    /// Authentication type 8
    pub const AUTH_TYPE_8: u64 = 1 << 26;
    /// Supports legacy pairing
    pub const LEGACY_PAIRING: u64 = 1 << 27;
    /// RAOP supported
    pub const RAOP: u64 = 1 << 28;
    /// Is carplay
    pub const IS_CARPLAY: u64 = 1 << 29;
    /// Control channel encryption
    pub const CONTROL_CHANNEL_ENCRYPT: u64 = 1 << 30;
    /// Supports unified media control
    pub const UNIFIED_MEDIA_CONTROL: u64 = 1 << 32;
    /// Supports buffered audio
    pub const BUFFERED_AUDIO: u64 = 1 << 38;
    /// Supports PTP clock
    pub const PTP_CLOCK: u64 = 1 << 40;
    /// Screen multi codec
    pub const SCREEN_MULTI_CODEC: u64 = 1 << 41;
    /// System pairing
    pub const SYSTEM_PAIRING: u64 = 1 << 43;
    /// Supports `AirPlay` 2 / APv2.5
    pub const AIRPLAY_2: u64 = 1 << 48;
    /// Supports system authentication
    pub const SYSTEM_AUTH: u64 = 1 << 49;
    /// Supports `CoreUtils` pairing and encryption
    pub const COREUTILS_PAIRING: u64 = 1 << 51;
    /// Supports transient pairing
    pub const TRANSIENT_PAIRING: u64 = 1 << 52;
}
