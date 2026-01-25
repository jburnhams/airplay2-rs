use super::raop::RaopCapabilities;
use std::collections::HashMap;
use std::net::IpAddr;

/// Represents a discovered `AirPlay` 2 device on the network
#[derive(Debug, Clone, PartialEq)]
pub struct AirPlayDevice {
    /// Unique device identifier (from TXT record)
    pub id: String,

    /// Human-readable device name (e.g., "Living Room `HomePod`")
    pub name: String,

    /// Device model identifier (e.g., "AudioAccessory5,1" for `HomePod` Mini)
    pub model: Option<String>,

    /// Resolved IP addresses
    pub addresses: Vec<IpAddr>,

    /// `AirPlay` service port
    pub port: u16,

    /// Device capabilities parsed from features flags
    pub capabilities: DeviceCapabilities,

    /// RAOP (`AirPlay` 1) service port
    pub raop_port: Option<u16>,

    /// RAOP capabilities parsed from TXT records
    pub raop_capabilities: Option<RaopCapabilities>,

    /// Raw TXT record data for protocol use
    pub txt_records: HashMap<String, String>,
}

/// Device capability flags parsed from `AirPlay` features
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeviceCapabilities {
    /// Supports `AirPlay` 2 protocol
    pub airplay2: bool,

    /// Supports multi-room/grouped playback
    pub supports_grouping: bool,

    /// Supports screen mirroring (not used, for info only)
    pub supports_screen: bool,

    /// Supports audio streaming
    pub supports_audio: bool,

    /// Supports high-resolution audio
    pub supports_hires_audio: bool,

    /// Supports buffered audio (for gapless playback)
    pub supports_buffered_audio: bool,

    /// Supports persistent pairing
    pub supports_persistent_pairing: bool,

    /// Supports `HomeKit` pairing
    pub supports_homekit_pairing: bool,

    /// Supports transient pairing
    pub supports_transient_pairing: bool,

    /// Raw features bitmask
    pub raw_features: u64,
}

impl AirPlayDevice {
    /// Check if this device supports `AirPlay` 2 features
    #[must_use]
    pub fn supports_airplay2(&self) -> bool {
        self.capabilities.airplay2
    }

    /// Check if this device supports RAOP (`AirPlay` 1)
    #[must_use]
    pub fn supports_raop(&self) -> bool {
        self.raop_port.is_some()
    }

    /// Check if this device can be part of a multi-room group
    #[must_use]
    pub fn supports_grouping(&self) -> bool {
        self.capabilities.supports_grouping
    }

    /// Get device volume if available from discovery
    #[must_use]
    pub fn discovered_volume(&self) -> Option<f32> {
        self.txt_records.get("vv").and_then(|v| v.parse().ok())
    }

    /// Get the primary IP address (prefers IPv4 for better connectivity)
    #[must_use]
    pub fn address(&self) -> IpAddr {
        // Prefer IPv4 addresses since IPv6 link-local addresses often have routing issues
        self.addresses
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| {
                // If no IPv4, try non-link-local IPv6
                self.addresses
                    .iter()
                    .find(|addr| matches!(addr, IpAddr::V6(v6) if v6.segments()[0] != 0xfe80))
            })
            .or_else(|| self.addresses.first())
            .copied()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    }
}

impl DeviceCapabilities {
    /// Parse capabilities from `AirPlay` features bitmask
    ///
    /// Features are documented at:
    /// <https://emanuelecozzi.net/docs/airplay2/features>
    #[must_use]
    pub fn from_features(features: u64) -> Self {
        Self {
            // Bit 9: Audio
            supports_audio: (features & (1 << 9)) != 0,
            // Bit 38: Supports buffered audio
            supports_buffered_audio: (features & (1 << 38)) != 0,
            // Bit 48: Supports AirPlay 2 / MFi authentication
            airplay2: (features & (1 << 48)) != 0,
            // Bit 32: Supports unified media control
            supports_grouping: (features & (1 << 32)) != 0,
            // Add other capability parsing...
            raw_features: features,
            ..Default::default()
        }
    }
}
