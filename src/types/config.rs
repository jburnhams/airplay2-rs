use crate::audio::AudioCodec;
use std::time::Duration;

/// Configuration for `AirPlay` client behavior
#[derive(Debug, Clone)]
pub struct AirPlayConfig {
    /// Timeout for device discovery scan (default: 5 seconds)
    pub discovery_timeout: Duration,

    /// Timeout for connection attempts (default: 10 seconds)
    pub connection_timeout: Duration,

    /// Interval for polling playback state (default: 500ms)
    pub state_poll_interval: Duration,

    /// Enable debug logging of protocol messages
    pub debug_protocol: bool,

    /// Number of reconnection attempts (default: 3)
    pub reconnect_attempts: u32,

    /// Delay between reconnection attempts (default: 1 second)
    pub reconnect_delay: Duration,

    /// Audio buffer size in frames (default: 44100 = 1 second at 44.1kHz)
    pub audio_buffer_frames: usize,

    /// Path to store persistent pairing keys (None = transient only)
    pub pairing_storage_path: Option<std::path::PathBuf>,

    /// Audio codec to use for streaming (default: PCM - uncompressed)
    pub audio_codec: AudioCodec,

    /// Optional PIN for pairing (if device requires one)
    pub pin: Option<String>,

    /// Bitrate for AAC encoding (bps) (default: `128_000`)
    pub aac_bitrate: u32,
}

impl Default for AirPlayConfig {
    fn default() -> Self {
        Self {
            discovery_timeout: Duration::from_secs(5),
            connection_timeout: Duration::from_secs(10),
            state_poll_interval: Duration::from_millis(500),
            debug_protocol: false,
            reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(1),
            audio_buffer_frames: 44100,
            pairing_storage_path: None,
            audio_codec: AudioCodec::Pcm, // Default to uncompressed PCM
            pin: None,
            aac_bitrate: 128_000,
        }
    }
}

impl AirPlayConfig {
    /// Create a new config builder
    #[must_use]
    pub fn builder() -> AirPlayConfigBuilder {
        AirPlayConfigBuilder::default()
    }
}

/// Builder for `AirPlayConfig`
#[derive(Debug, Clone, Default)]
pub struct AirPlayConfigBuilder {
    config: AirPlayConfig,
}

impl AirPlayConfigBuilder {
    /// Set discovery timeout
    #[must_use]
    pub fn discovery_timeout(mut self, timeout: Duration) -> Self {
        self.config.discovery_timeout = timeout;
        self
    }

    /// Set connection timeout
    #[must_use]
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Set state polling interval
    #[must_use]
    pub fn state_poll_interval(mut self, interval: Duration) -> Self {
        self.config.state_poll_interval = interval;
        self
    }

    /// Enable protocol debug logging
    #[must_use]
    pub fn debug_protocol(mut self, enable: bool) -> Self {
        self.config.debug_protocol = enable;
        self
    }

    /// Set pairing storage path for persistent pairing
    #[must_use]
    pub fn pairing_storage(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config.pairing_storage_path = Some(path.into());
        self
    }

    /// Set audio codec for streaming (PCM or ALAC)
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.config.audio_codec = codec;
        self
    }

    /// Set PIN for pairing
    #[must_use]
    pub fn pin(mut self, pin: impl Into<String>) -> Self {
        self.config.pin = Some(pin.into());
        self
    }

    /// Set AAC bitrate in bits per second (default: `128_000`)
    #[must_use]
    pub fn aac_bitrate(mut self, bitrate: u32) -> Self {
        self.config.aac_bitrate = bitrate;
        self
    }

    /// Build the configuration
    #[must_use]
    pub fn build(self) -> AirPlayConfig {
        self.config
    }
}
