//! DACP command definitions

/// DACP playback commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DacpCommand {
    /// Start/resume playback
    Play,
    /// Pause playback
    Pause,
    /// Toggle play/pause
    PlayPause,
    /// Resume from pause
    PlayResume,
    /// Stop playback
    Stop,
    /// Skip to next track
    NextItem,
    /// Go to previous track
    PrevItem,
    /// Begin fast forward
    BeginFastForward,
    /// Begin rewind
    BeginRewind,
    /// End fast forward/rewind
    PlayResume2,
    /// Increase volume
    VolumeUp,
    /// Decrease volume
    VolumeDown,
    /// Toggle mute
    MuteToggle,
    /// Shuffle songs
    ShuffleSongs,
}

impl DacpCommand {
    /// Parse from URL path
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        // Path format: /ctrl-int/1/{command}
        let command = path.strip_prefix("/ctrl-int/1/")?;

        match command {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "playpause" => Some(Self::PlayPause),
            "playresume" => Some(Self::PlayResume),
            "stop" => Some(Self::Stop),
            "nextitem" => Some(Self::NextItem),
            "previtem" => Some(Self::PrevItem),
            "beginff" => Some(Self::BeginFastForward),
            "beginrew" => Some(Self::BeginRewind),
            "volumeup" => Some(Self::VolumeUp),
            "volumedown" => Some(Self::VolumeDown),
            "mutetoggle" => Some(Self::MuteToggle),
            "shuffle_songs" => Some(Self::ShuffleSongs),
            _ => None,
        }
    }

    /// Get URL path for command
    #[must_use]
    pub fn path(&self) -> &'static str {
        match self {
            Self::Play => "/ctrl-int/1/play",
            Self::Pause => "/ctrl-int/1/pause",
            Self::PlayPause => "/ctrl-int/1/playpause",
            Self::PlayResume | Self::PlayResume2 => "/ctrl-int/1/playresume",
            Self::Stop => "/ctrl-int/1/stop",
            Self::NextItem => "/ctrl-int/1/nextitem",
            Self::PrevItem => "/ctrl-int/1/previtem",
            Self::BeginFastForward => "/ctrl-int/1/beginff",
            Self::BeginRewind => "/ctrl-int/1/beginrew",
            Self::VolumeUp => "/ctrl-int/1/volumeup",
            Self::VolumeDown => "/ctrl-int/1/volumedown",
            Self::MuteToggle => "/ctrl-int/1/mutetoggle",
            Self::ShuffleSongs => "/ctrl-int/1/shuffle_songs",
        }
    }

    /// Get human-readable description
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::PlayPause => "Play/Pause",
            Self::PlayResume | Self::PlayResume2 => "Resume",
            Self::Stop => "Stop",
            Self::NextItem => "Next Track",
            Self::PrevItem => "Previous Track",
            Self::BeginFastForward => "Fast Forward",
            Self::BeginRewind => "Rewind",
            Self::VolumeUp => "Volume Up",
            Self::VolumeDown => "Volume Down",
            Self::MuteToggle => "Toggle Mute",
            Self::ShuffleSongs => "Shuffle",
        }
    }
}

/// Result of command execution
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command executed successfully
    Success,
    /// Command not supported
    NotSupported,
    /// Command failed
    Failed(String),
}
