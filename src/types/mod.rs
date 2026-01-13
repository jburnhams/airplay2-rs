//! Core types module

/// Represents an `AirPlay` device found on the network.
#[derive(Debug, Clone)]
pub struct AirPlayDevice {
    /// The friendly name of the device.
    pub name: String,
    // Add other fields as needed
}

/// Configuration for the `AirPlay` library.
#[derive(Debug, Clone, Default)]
pub struct AirPlayConfig {
    /// Timeout for device discovery.
    pub discovery_timeout: std::time::Duration,
    /// Timeout for establishing a connection.
    pub connection_timeout: std::time::Duration,
    /// Interval for polling playback state.
    pub state_poll_interval: std::time::Duration,
    /// Whether to log debug protocol information.
    pub debug_protocol: bool,
}

/// Information about a track to play.
#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    /// The URL of the track.
    pub url: String,
    /// The title of the track.
    pub title: String,
    /// The artist of the track.
    pub artist: String,
}

/// The current state of playback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    /// Audio is currently playing.
    Playing,
    /// Playback is paused.
    Paused,
    /// Playback is stopped.
    Stopped,
}

/// Repeat mode for the playback queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatMode {
    /// No repeat.
    Off,
    /// Repeat the current track.
    One,
    /// Repeat the entire queue.
    All,
}

/// Information about the current playback status.
#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    // Add fields
}
