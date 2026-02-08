//! Configuration for AirPlay 2 Receiver

use crate::types::raop::RaopCodec as AudioFormat;
use rand::Rng;

/// Configuration for an `AirPlay` 2 receiver instance
#[derive(Debug, Clone)]
pub struct Ap2Config {
    /// Device name (shown to senders)
    pub name: String,

    /// Unique device ID (typically MAC address format: AA:BB:CC:DD:EE:FF)
    pub device_id: String,

    /// Model identifier (e.g., "Receiver1,1")
    pub model: String,

    /// Manufacturer name
    pub manufacturer: String,

    /// Serial number (optional)
    pub serial_number: Option<String>,

    /// Firmware version
    pub firmware_version: String,

    /// RTSP/HTTP server port (default: 7000)
    pub server_port: u16,

    /// Enable password authentication
    pub password: Option<String>,

    /// Supported audio formats
    pub audio_formats: Vec<AudioFormat>,

    /// Enable multi-room support (feature bit 40)
    pub multi_room_enabled: bool,

    /// Audio buffer size in milliseconds
    pub buffer_size_ms: u32,

    /// Maximum concurrent sessions (usually 1)
    pub max_sessions: usize,

    /// Enable verbose protocol logging
    pub debug_logging: bool,
}

impl Default for Ap2Config {
    fn default() -> Self {
        Self {
            name: "AirPlay Receiver".to_string(),
            device_id: Self::generate_device_id(),
            model: "Receiver1,1".to_string(),
            manufacturer: "airplay2-rs".to_string(),
            serial_number: None,
            firmware_version: env!("CARGO_PKG_VERSION").to_string(),
            server_port: 7000,
            password: None,
            audio_formats: vec![AudioFormat::Pcm, AudioFormat::Alac, AudioFormat::AacEld],
            multi_room_enabled: true,
            buffer_size_ms: 2000,
            max_sessions: 1,
            debug_logging: false,
        }
    }
}

impl Ap2Config {
    /// Create a new configuration with the given device name
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set password protection
    #[must_use]
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Disable multi-room support
    #[must_use]
    pub fn without_multi_room(mut self) -> Self {
        self.multi_room_enabled = false;
        self
    }

    /// Set custom server port
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.server_port = port;
        self
    }

    /// Generate a random device ID in MAC address format
    fn generate_device_id() -> String {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 6] = rng.r#gen();
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
        )
    }

    /// Calculate feature flags based on configuration
    #[must_use]
    pub fn feature_flags(&self) -> u64 {
        let mut flags: u64 = 0;

        // Core features (always enabled)
        flags |= 1 << 0; // Video supported (even if we only do audio)
        flags |= 1 << 1; // Photo supported
        flags |= 1 << 7; // Audio
        flags |= 1 << 9; // Audio redundant (FEC)
        flags |= 1 << 14; // MFi soft auth
        flags |= 1 << 17; // Supports pairing
        flags |= 1 << 18; // Supports PIN pairing
        flags |= 1 << 27; // Supports unified media control

        // Optional features
        if self.multi_room_enabled {
            flags |= 1 << 40; // Buffered audio
            flags |= 1 << 41; // PTP clock
            flags |= 1 << 46; // HomeKit pairing
        }

        if self.password.is_some() {
            flags |= 1 << 15; // Password required
        }

        flags
    }

    /// Get status flags for TXT record
    #[must_use]
    pub fn status_flags(&self) -> u32 {
        let mut flags: u32 = 0;

        // Bit 2: Problem detected (0 = no problem)
        // Bit 3: Supports PIN (1 = yes)
        flags |= 1 << 3;

        // Bit 4: Supports password
        if self.password.is_some() {
            flags |= 1 << 4;
        }

        flags
    }
}

/// Builder for `Ap2Config` with validation
pub struct Ap2ConfigBuilder {
    config: Ap2Config,
}

impl Ap2ConfigBuilder {
    /// Create a new builder with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Ap2Config::default(),
        }
    }

    /// Set the device name
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    /// Set the device ID (MAC address format)
    #[must_use]
    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.config.device_id = id.into();
        self
    }

    /// Set password protection
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.config.password = Some(password.into());
        self
    }

    /// Set the server port
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    pub fn buffer_size_ms(mut self, ms: u32) -> Self {
        self.config.buffer_size_ms = ms;
        self
    }

    /// Disable multi-room support
    #[must_use]
    pub fn without_multi_room(mut self) -> Self {
        self.config.multi_room_enabled = false;
        self
    }

    /// Build the configuration
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if validation fails:
    /// - Name is empty
    /// - Device ID is invalid (length != 17)
    /// - Port is 0
    pub fn build(self) -> Result<Ap2Config, ConfigError> {
        // Validate configuration
        if self.config.name.is_empty() {
            return Err(ConfigError::InvalidName("Name cannot be empty".into()));
        }

        if self.config.device_id.len() != 17 {
            return Err(ConfigError::InvalidDeviceId(
                "Device ID must be in MAC address format".into(),
            ));
        }

        if self.config.server_port == 0 {
            return Err(ConfigError::InvalidPort("Port cannot be 0".into()));
        }

        Ok(self.config)
    }
}

impl Default for Ap2ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Device name is invalid
    #[error("Invalid device name: {0}")]
    InvalidName(String),

    /// Device ID is invalid (must be MAC address format)
    #[error("Invalid device ID: {0}")]
    InvalidDeviceId(String),

    /// Port is invalid
    #[error("Invalid port: {0}")]
    InvalidPort(String),
}
