//! Receiver events for UI and application integration

use super::artwork_handler::Artwork;
use super::metadata_handler::TrackMetadata;
use super::progress_handler::PlaybackProgress;
use std::net::SocketAddr;

/// Events emitted by the receiver
#[derive(Debug, Clone)]
pub enum ReceiverEvent {
    /// Receiver started and advertising
    Started {
        /// Receiver name
        name: String,
        /// Listen port
        port: u16,
    },

    /// Receiver stopped
    Stopped,

    /// Client connected
    ClientConnected {
        /// Client address
        address: SocketAddr,
        /// User agent string
        user_agent: Option<String>,
    },

    /// Client disconnected
    ClientDisconnected {
        /// Client address
        address: SocketAddr,
        /// Disconnect reason
        reason: String,
    },

    /// Playback started
    PlaybackStarted,

    /// Playback paused
    PlaybackPaused,

    /// Playback stopped
    PlaybackStopped,

    /// Volume changed
    VolumeChanged {
        /// Volume in dB (-144 to 0)
        db: f32,
        /// Linear volume (0.0 to 1.0)
        linear: f32,
        /// Is muted
        muted: bool,
    },

    /// Track metadata updated
    MetadataUpdated(TrackMetadata),

    /// Artwork updated
    ArtworkUpdated(Artwork),

    /// Progress updated
    ProgressUpdated(PlaybackProgress),

    /// Buffer status changed
    BufferStatus {
        /// Buffer fill percentage
        fill: f64,
        /// Is underrunning
        underrun: bool,
    },

    /// Error occurred
    Error {
        /// Error message
        message: String,
        /// Is error recoverable
        recoverable: bool,
    },
}

/// Callback type for receiver events
pub type EventCallback = Box<dyn Fn(ReceiverEvent) + Send + Sync + 'static>;
