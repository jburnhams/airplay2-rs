//! AirPlay 2 Feature Flags
//!
//! Feature flags advertise receiver capabilities to senders.
//! They are transmitted as a 64-bit value in the TXT record.

/// Feature flag bit positions
///
/// These are the known feature flags for AirPlay 2. The complete
/// list is not publicly documented by Apple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FeatureFlag {
    /// Bit 0: Video playback support
    Video = 0,
    /// Bit 1: Photo display support
    Photo = 1,
    /// Bit 2: Video fair play (DRM)
    VideoFairPlay = 2,
    /// Bit 3: Video volume control
    VideoVolumeControl = 3,
    /// Bit 4: Video HTTP live streaming
    VideoHttpLiveStreaming = 4,
    /// Bit 5: Slideshow
    Slideshow = 5,
    /// Bit 6: Unknown/reserved
    Reserved6 = 6,
    /// Bit 7: Screen mirroring
    ScreenMirroring = 7,
    /// Bit 8: Screen rotation
    ScreenRotation = 8,
    /// Bit 9: Audio (core audio streaming)
    Audio = 9,
    /// Bit 10: Unknown/reserved
    Reserved10 = 10,
    /// Bit 11: Audio redundant (FEC/retransmission)
    AudioRedundant = 11,
    /// Bit 12: FairPlay secure auth
    FairPlaySecureAuth = 12,
    /// Bit 13: Photo caching
    PhotoCaching = 13,
    /// Bit 14: Authentication setup (MFi soft)
    AuthenticationSetup = 14,
    /// Bit 15: Metadata features (bit 1)
    MetadataFeatures1 = 15,
    /// Bit 16: Metadata features (bit 2)
    MetadataFeatures2 = 16,
    /// Bit 17: Legacy pairing support
    LegacyPairing = 17,
    /// Bit 18: Unified media control
    UnifiedMediaControl = 18,
    /// Bit 19: Supports volume control (RAOP)
    SupportsVolume = 19,
    /// Bit 20: Remote control relay
    RemoteControlRelay = 20,
    /// Bit 22: Audio format - ALAC
    AudioFormatAlac = 22,
    /// Bit 23: Audio format - AAC-LC
    AudioFormatAacLc = 23,
    /// Bit 25: Audio format - AAC-ELD
    AudioFormatAacEld = 25,
    /// Bit 26: Supports PIN pairing
    SupportsPin = 26,
    /// Bit 27: Supports transient pairing
    SupportsTransientPairing = 27,
    /// Bit 30: Supports system pairing
    SupportsSystemPairing = 30,
    /// Bit 32: Is speaker (group leader)
    IsSpeaker = 32,
    /// Bit 38: Supports buffered audio (for multi-room)
    SupportsBufferedAudio = 38,
    /// Bit 40: Supports PTP clock sync
    SupportsPtp = 40,
    /// Bit 41: Supports screen mirroring 2
    SupportsScreenMirroring2 = 41,
    /// Bit 42: Supports unified pair setup/verify
    SupportsUnifiedPairSetupAndVerify = 42,
    /// Bit 46: Supports HomeKit pairing
    SupportsHomeKit = 46,
    /// Bit 48: Supports CoreUtils pairing
    SupportsCoreUtilsPairing = 48,
    /// Bit 50: Supports persistent credentials
    SupportsPersistentCredentials = 50,
    /// Bit 51: Supports AirPlay video v2
    SupportsAirPlayVideoV2 = 51,
    /// Bit 52: Audio meta-data via TXT record
    AudioMetadataTxtRecord = 52,
    /// Bit 54: Supports unified advertising
    SupportsUnifiedAdvertising = 54,
}

impl FeatureFlag {
    /// Convert to bit mask
    pub fn mask(&self) -> u64 {
        1u64 << (*self as u8)
    }
}

/// Feature flag set builder
#[derive(Debug, Clone, Default)]
pub struct FeatureFlags {
    flags: u64,
}

impl FeatureFlags {
    /// Create empty feature set
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    /// Create default feature set for audio-only receiver
    pub fn audio_receiver() -> Self {
        let mut flags = Self::new();

        // Core audio features
        flags.set(FeatureFlag::Audio);
        flags.set(FeatureFlag::AudioRedundant);
        flags.set(FeatureFlag::SupportsVolume);

        // Audio formats
        flags.set(FeatureFlag::AudioFormatAlac);
        flags.set(FeatureFlag::AudioFormatAacLc);
        flags.set(FeatureFlag::AudioFormatAacEld);

        // Authentication
        flags.set(FeatureFlag::AuthenticationSetup);
        flags.set(FeatureFlag::LegacyPairing);
        flags.set(FeatureFlag::SupportsPin);
        flags.set(FeatureFlag::SupportsTransientPairing);
        flags.set(FeatureFlag::SupportsHomeKit);

        // Metadata
        flags.set(FeatureFlag::MetadataFeatures1);
        flags.set(FeatureFlag::MetadataFeatures2);

        flags
    }

    /// Create feature set for multi-room capable receiver
    pub fn multi_room_receiver() -> Self {
        let mut flags = Self::audio_receiver();

        flags.set(FeatureFlag::SupportsBufferedAudio);
        flags.set(FeatureFlag::SupportsPtp);
        flags.set(FeatureFlag::IsSpeaker);

        flags
    }

    /// Set a feature flag
    pub fn set(&mut self, flag: FeatureFlag) -> &mut Self {
        self.flags |= flag.mask();
        self
    }

    /// Clear a feature flag
    pub fn clear(&mut self, flag: FeatureFlag) -> &mut Self {
        self.flags &= !flag.mask();
        self
    }

    /// Check if a feature flag is set
    pub fn has(&self, flag: FeatureFlag) -> bool {
        (self.flags & flag.mask()) != 0
    }

    /// Get raw flags value
    pub fn raw(&self) -> u64 {
        self.flags
    }

    /// Format for TXT record (two 32-bit hex values)
    pub fn to_txt_value(&self) -> String {
        format!(
            "0x{:X},0x{:X}",
            self.flags & 0xFFFFFFFF,
            self.flags >> 32
        )
    }

    /// Parse from TXT record value
    pub fn from_txt_value(value: &str) -> Option<Self> {
        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() != 2 {
            return None;
        }

        let low = u32::from_str_radix(parts[0].trim_start_matches("0x"), 16).ok()?;
        let high = u32::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;

        Some(Self {
            flags: (low as u64) | ((high as u64) << 32),
        })
    }
}

/// Status flags for the `flags` TXT record field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StatusFlag {
    /// Bit 0: Problem detected
    ProblemDetected = 0,
    /// Bit 1: Not yet configured
    NotConfigured = 1,
    /// Bit 2: Audio cable attached
    AudioCableAttached = 2,
    /// Bit 3: Supports PIN
    SupportsPin = 3,
    /// Bit 4: Requires password
    RequiresPassword = 4,
    /// Bit 5: Password set (but may not be required)
    PasswordSet = 5,
    /// Bit 6: Device locked
    DeviceLocked = 6,
    /// Bit 11: Accessory problems
    AccessoryProblems = 11,
}

impl StatusFlag {
    /// Get the bit mask for this flag
    pub fn mask(&self) -> u32 {
        1u32 << (*self as u8)
    }
}

/// Set of status flags
#[derive(Debug, Clone, Default)]
pub struct StatusFlags {
    flags: u32,
}

impl StatusFlags {
    /// Create a new empty status flags set
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    /// Default status for a working receiver
    pub fn healthy() -> Self {
        let mut flags = Self::new();
        flags.set(StatusFlag::SupportsPin);
        flags
    }

    /// Status when password is configured
    pub fn with_password() -> Self {
        let mut flags = Self::healthy();
        flags.set(StatusFlag::RequiresPassword);
        flags.set(StatusFlag::PasswordSet);
        flags
    }

    /// Set a status flag
    pub fn set(&mut self, flag: StatusFlag) -> &mut Self {
        self.flags |= flag.mask();
        self
    }

    /// Clear a status flag
    pub fn clear(&mut self, flag: StatusFlag) -> &mut Self {
        self.flags &= !flag.mask();
        self
    }

    /// Check if a status flag is set
    pub fn has(&self, flag: StatusFlag) -> bool {
        (self.flags & flag.mask()) != 0
    }

    /// Get raw flags value
    pub fn raw(&self) -> u32 {
        self.flags
    }

    /// Format for TXT record (32-bit hex value)
    pub fn to_txt_value(&self) -> String {
        format!("0x{:X}", self.flags)
    }
}
