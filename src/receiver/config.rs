//! `AirPlay` receiver configuration

use std::time::Duration;

use crate::discovery::advertiser::RaopCapabilities;

/// Receiver configuration
#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    /// Device name shown to senders
    pub name: String,

    /// RTSP listen port (0 = auto-assign)
    pub port: u16,

    /// Receiver capabilities
    pub capabilities: RaopCapabilities,

    /// Session timeout
    pub session_timeout: Duration,

    /// Allow session preemption
    pub allow_preemption: bool,

    /// Target audio latency in milliseconds
    pub latency_ms: u32,

    /// Jitter buffer configuration
    pub jitter_buffer_depth: usize,

    /// Audio output device (None = default)
    pub audio_device: Option<String>,

    /// Initial volume (0.0 to 1.0)
    pub initial_volume: f32,

    /// Enable debug logging
    pub debug: bool,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            name: "AirPlay Receiver".to_string(),
            port: 5000,
            capabilities: RaopCapabilities::default(),
            session_timeout: Duration::from_secs(60),
            allow_preemption: true,
            latency_ms: 2000,
            jitter_buffer_depth: 50,
            audio_device: None,
            initial_volume: 1.0,
            debug: false,
        }
    }
}

impl ReceiverConfig {
    /// Create with custom name
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set port
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set latency
    #[must_use]
    pub fn latency_ms(mut self, ms: u32) -> Self {
        self.latency_ms = ms;
        self
    }

    /// Set audio device
    #[must_use]
    pub fn audio_device(mut self, device: impl Into<String>) -> Self {
        self.audio_device = Some(device.into());
        self
    }
}
