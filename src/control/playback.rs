//! Playback control for `AirPlay`

use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::protocol::rtsp::Method;
use crate::types::{PlaybackState, RepeatMode};

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Shuffle mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShuffleMode {
    /// Shuffle off
    #[default]
    Off,
    /// Shuffle on
    On,
}

/// Playback controller
pub struct PlaybackController {
    /// Connection manager
    connection: Arc<ConnectionManager>,
    /// Current playback state
    state: RwLock<PlaybackState>,
    /// Current repeat mode
    repeat_mode: RwLock<RepeatMode>,
    /// Current shuffle mode
    shuffle_mode: RwLock<ShuffleMode>,
}

impl PlaybackController {
    /// Create a new playback controller
    #[must_use]
    pub fn new(connection: Arc<ConnectionManager>) -> Self {
        Self {
            connection,
            state: RwLock::new(PlaybackState::default()),
            repeat_mode: RwLock::new(RepeatMode::Off),
            shuffle_mode: RwLock::new(ShuffleMode::Off),
        }
    }

    /// Get current playback state
    pub async fn state(&self) -> PlaybackState {
        self.state.read().await.clone()
    }

    /// Play (resume if paused, start if stopped)
    ///
    /// # Errors
    ///
    /// Returns error if state is invalid or network fails
    pub async fn play(&self) -> Result<(), AirPlayError> {
        let mut state = self.state.write().await;

        if state.is_playing {
            // Already playing
            return Ok(());
        }

        if state.current_track.is_none() && state.queue.is_empty() {
            return Err(AirPlayError::InvalidState {
                message: "No content to play".to_string(),
                current_state: "Stopped".to_string(),
            });
        }

        // Send resume command (Method::Play)
        self.connection
            .send_command(Method::Play, None, None)
            .await?;

        state.is_playing = true;
        Ok(())
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        let mut state = self.state.write().await;

        if state.is_playing {
            self.connection
                .send_command(Method::Pause, None, None)
                .await?;
            state.is_playing = false;
        }

        Ok(())
    }

    /// Toggle play/pause
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn toggle(&self) -> Result<(), AirPlayError> {
        let is_playing = self.state.read().await.is_playing;
        if is_playing {
            self.pause().await
        } else {
            self.play().await
        }
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.connection
            .send_command(Method::Teardown, None, None)
            .await?;

        let mut state = self.state.write().await;
        state.is_playing = false;
        state.position_secs = 0.0;
        // Keep track/queue for now, as stop doesn't necessarily clear queue in some players

        Ok(())
    }

    /// Skip to next track
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn next(&self) -> Result<(), AirPlayError> {
        self.send_command("nextitem").await
    }

    /// Go to previous track
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn previous(&self) -> Result<(), AirPlayError> {
        self.send_command("previtem").await
    }

    /// Seek to position
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.send_scrub(position.as_secs_f64()).await?;

        let mut state = self.state.write().await;
        state.position_secs = position.as_secs_f64();
        Ok(())
    }

    /// Seek relative to current position
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn seek_relative(&self, offset: Duration, forward: bool) -> Result<(), AirPlayError> {
        // Read current state to calculate new position
        // We accept a small race condition here to avoid holding lock during network op
        let current_pos = self.state.read().await.position_secs;

        let new_pos = if forward {
            current_pos + offset.as_secs_f64()
        } else {
            (current_pos - offset.as_secs_f64()).max(0.0)
        };

        self.send_scrub(new_pos).await?;

        // Update state
        let mut state = self.state.write().await;
        state.position_secs = new_pos;
        Ok(())
    }

    /// Fast forward
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn fast_forward(&self) -> Result<(), AirPlayError> {
        // TODO: Implement rate control properly
        // For now just skip forward 10s
        self.seek_relative(Duration::from_secs(10), true).await
    }

    /// Rewind
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn rewind(&self) -> Result<(), AirPlayError> {
        // TODO: Implement rate control properly
        // For now just skip backward 10s
        self.seek_relative(Duration::from_secs(10), false).await
    }

    /// Set repeat mode
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), AirPlayError> {
        self.send_command(match mode {
            RepeatMode::Off => "repeatoff",
            RepeatMode::One => "repeatone",
            RepeatMode::All => "repeatall",
        })
        .await?;

        let mut state = self.state.write().await;
        state.repeat = mode;
        *self.repeat_mode.write().await = mode;
        Ok(())
    }

    /// Get repeat mode
    pub async fn repeat_mode(&self) -> RepeatMode {
        *self.repeat_mode.read().await
    }

    /// Set shuffle mode
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_shuffle(&self, mode: ShuffleMode) -> Result<(), AirPlayError> {
        self.send_command(match mode {
            ShuffleMode::Off => "shuffleoff",
            ShuffleMode::On => "shuffleon",
        })
        .await?;

        let mut state = self.state.write().await;
        state.shuffle = matches!(mode, ShuffleMode::On);
        *self.shuffle_mode.write().await = mode;
        Ok(())
    }

    /// Get shuffle mode
    pub async fn shuffle_mode(&self) -> ShuffleMode {
        *self.shuffle_mode.read().await
    }

    /// Internal: send scrub command
    #[allow(clippy::unused_async)]
    async fn send_scrub(&self, _position: f64) -> Result<(), AirPlayError> {
        // TODO: Send scrub command (e.g., SET_PARAMETER with progress)
        Err(AirPlayError::NotImplemented {
            feature: "scrub/seek".to_string(),
        })
    }

    /// Internal: send generic command (usually DACP)
    async fn send_command(&self, command: &str) -> Result<(), AirPlayError> {
        // Attempt to map to DACP path
        let path = format!("/ctrl-int/1/{command}");

        // We use send_post_command.
        let _ = self.connection.send_post_command(&path, None, None).await?;
        Ok(())
    }
}

/// Playback progress information
#[derive(Debug, Clone)]
pub struct PlaybackProgress {
    /// Current position
    pub position: Duration,
    /// Total duration
    pub duration: Duration,
    /// Current rate (1.0 = normal, 0.0 = paused)
    pub rate: f32,
}

impl PlaybackProgress {
    /// Get progress as percentage (0.0 - 1.0)
    #[must_use]
    pub fn progress(&self) -> f64 {
        if self.duration.is_zero() {
            0.0
        } else {
            self.position.as_secs_f64() / self.duration.as_secs_f64()
        }
    }

    /// Get remaining time
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.duration.saturating_sub(self.position)
    }
}
