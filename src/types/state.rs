use super::track::{TrackInfo, QueueItem};

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

    /// Queue items with unique IDs: (track, item_id)
    pub items: Vec<(TrackInfo, i32)>,
}

impl From<&PlaybackState> for PlaybackInfo {
    fn from(state: &PlaybackState) -> Self {
        Self {
            current_track: state.current_track.clone(),
            index: state.queue_index.map_or(0, |i| i as u32),
            position_ms: (state.position_secs * 1000.0) as u32,
            is_playing: state.is_playing,
            items: state
                .queue
                .iter()
                .map(|item| (item.track.clone(), item.item_id))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_state_default() {
        let state = PlaybackState::default();
        assert!(!state.is_playing);
        assert!(state.current_track.is_none());
        assert_eq!(state.volume, 0.0);
        assert_eq!(state.repeat, RepeatMode::Off);
    }

    #[test]
    fn test_playback_info_from_state() {
        let mut state = PlaybackState::default();
        state.position_secs = 30.5;
        state.is_playing = true;

        let info = PlaybackInfo::from(&state);

        assert_eq!(info.position_ms, 30500);
        assert!(info.is_playing);
    }

    #[test]
    fn test_repeat_mode_equality() {
        assert_eq!(RepeatMode::Off, RepeatMode::Off);
        assert_ne!(RepeatMode::Off, RepeatMode::All);
    }
}
