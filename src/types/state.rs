use super::track::{QueueItem, TrackInfo};

/// Current playback state of a connected device
#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    /// Whether audio is currently playing
    pub is_playing: bool,

    /// Current track info (None if queue empty)
    pub current_track: Option<TrackInfo>,

    /// Position in current track (seconds)
    pub position_secs: f64,

    /// Duration of current track (seconds)
    pub duration_secs: Option<f64>,

    /// Current volume (0.0 - 1.0)
    pub volume: f32,

    /// Current queue
    pub queue: Vec<QueueItem>,

    /// Index of current track in queue
    pub queue_index: Option<usize>,

    /// Whether shuffle is enabled
    pub shuffle: bool,

    /// Current repeat mode
    pub repeat: RepeatMode,

    /// Connection state
    pub connection_state: ConnectionState,
}

/// Repeat mode for queue playback
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepeatMode {
    /// No repeat
    #[default]
    Off,
    /// Repeat entire queue
    All,
    /// Repeat current track
    One,
}

/// Connection state of the client
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// Connection in progress
    Connecting,
    /// Pairing/authenticating
    Pairing,
    /// Connected and ready
    Connected,
    /// Connection lost, attempting reconnect
    Reconnecting,
}

/// Playback info matching music-player integration requirements
#[derive(Debug, Clone, Default)]
pub struct PlaybackInfo {
    /// Currently playing track
    pub current_track: Option<TrackInfo>,

    /// Index in queue
    pub index: u32,

    /// Position in milliseconds
    pub position_ms: u32,

    /// Whether currently playing
    pub is_playing: bool,

    /// Queue items with unique IDs: (track, `item_id`)
    pub items: Vec<(TrackInfo, i32)>,
}

impl From<&PlaybackState> for PlaybackInfo {
    fn from(state: &PlaybackState) -> Self {
        Self {
            current_track: state.current_track.clone(),
            // Queue index is likely small enough to fit in u32
            index: state
                .queue_index
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(0),
            // Position is in seconds, convert to ms. u32 holds ~1193 hours of ms.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "Position in seconds fits in u32 ms"
            )]
            position_ms: (state.position_secs * 1000.0) as u32,
            is_playing: state.is_playing,
            items: state
                .queue
                .iter()
                .map(|item| {
                    // The cast from u64 to i32 is unsafe. Using try_from is safer.
                    (item.track.clone(), i32::try_from(item.id.0).unwrap_or(-1))
                })
                .collect(),
        }
    }
}
