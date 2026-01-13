# Section 15: Playback Control

## Dependencies
- **Section 02**: Core Types (must be complete)
- **Section 05**: RTSP Protocol (must be complete)
- **Section 10**: Connection Management (must be complete)

## Overview

This section provides playback control commands including:
- Play/Pause/Stop
- Previous/Next track
- Seek/Scrub
- Shuffle/Repeat modes

## Objectives

- Implement playback control commands
- Track playback state locally
- Handle device feedback
- Support transport controls

---

## Tasks

### 15.1 Playback Controller

- [ ] **15.1.1** Implement playback control interface

**File:** `src/control/playback.rs`

```rust
//! Playback control for AirPlay

use crate::types::PlaybackState;
use crate::connection::ConnectionManager;
use crate::error::AirPlayError;

use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// Repeat mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    /// No repeat
    Off,
    /// Repeat current track
    One,
    /// Repeat entire queue
    All,
}

/// Shuffle mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShuffleMode {
    /// Shuffle off
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
    /// Current volume (0.0 - 1.0)
    volume: RwLock<f32>,
}

impl PlaybackController {
    /// Create a new playback controller
    pub fn new(connection: Arc<ConnectionManager>) -> Self {
        Self {
            connection,
            state: RwLock::new(PlaybackState::Stopped),
            repeat_mode: RwLock::new(RepeatMode::Off),
            shuffle_mode: RwLock::new(ShuffleMode::Off),
            volume: RwLock::new(1.0),
        }
    }

    /// Get current playback state
    pub async fn state(&self) -> PlaybackState {
        *self.state.read().await
    }

    /// Play (resume if paused, start if stopped)
    pub async fn play(&self) -> Result<(), AirPlayError> {
        let current = *self.state.read().await;

        match current {
            PlaybackState::Paused => {
                // Send resume command
                self.send_rate(1.0).await?;
                *self.state.write().await = PlaybackState::Playing;
            }
            PlaybackState::Stopped => {
                // Need to start playback from queue
                return Err(AirPlayError::InvalidState {
                    message: "No content to play".to_string(),
                    current_state: "Stopped".to_string(),
                });
            }
            PlaybackState::Playing => {
                // Already playing
            }
            _ => {}
        }

        Ok(())
    }

    /// Pause playback
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        let current = *self.state.read().await;

        if current == PlaybackState::Playing {
            self.send_rate(0.0).await?;
            *self.state.write().await = PlaybackState::Paused;
        }

        Ok(())
    }

    /// Toggle play/pause
    pub async fn toggle(&self) -> Result<(), AirPlayError> {
        let current = *self.state.read().await;

        match current {
            PlaybackState::Playing => self.pause().await,
            PlaybackState::Paused => self.play().await,
            _ => Ok(()),
        }
    }

    /// Stop playback
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.send_stop().await?;
        *self.state.write().await = PlaybackState::Stopped;
        Ok(())
    }

    /// Skip to next track
    pub async fn next(&self) -> Result<(), AirPlayError> {
        self.send_command("nextitem").await
    }

    /// Go to previous track
    pub async fn previous(&self) -> Result<(), AirPlayError> {
        self.send_command("previtem").await
    }

    /// Seek to position
    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.send_scrub(position.as_secs_f64()).await
    }

    /// Seek relative to current position
    pub async fn seek_relative(&self, offset: Duration, forward: bool) -> Result<(), AirPlayError> {
        // Get current position, calculate new position, seek
        // For now, this is a placeholder
        Ok(())
    }

    /// Fast forward
    pub async fn fast_forward(&self) -> Result<(), AirPlayError> {
        self.send_rate(2.0).await
    }

    /// Rewind
    pub async fn rewind(&self) -> Result<(), AirPlayError> {
        self.send_rate(-2.0).await
    }

    /// Set repeat mode
    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), AirPlayError> {
        self.send_command(match mode {
            RepeatMode::Off => "repeatoff",
            RepeatMode::One => "repeatone",
            RepeatMode::All => "repeatall",
        }).await?;

        *self.repeat_mode.write().await = mode;
        Ok(())
    }

    /// Get repeat mode
    pub async fn repeat_mode(&self) -> RepeatMode {
        *self.repeat_mode.read().await
    }

    /// Set shuffle mode
    pub async fn set_shuffle(&self, mode: ShuffleMode) -> Result<(), AirPlayError> {
        self.send_command(match mode {
            ShuffleMode::Off => "shuffleoff",
            ShuffleMode::On => "shuffleon",
        }).await?;

        *self.shuffle_mode.write().await = mode;
        Ok(())
    }

    /// Get shuffle mode
    pub async fn shuffle_mode(&self) -> ShuffleMode {
        *self.shuffle_mode.read().await
    }

    /// Internal: send rate command
    async fn send_rate(&self, rate: f32) -> Result<(), AirPlayError> {
        // TODO: Send RTSP SET_PARAMETER with rate
        Ok(())
    }

    /// Internal: send scrub command
    async fn send_scrub(&self, position: f64) -> Result<(), AirPlayError> {
        // TODO: Send scrub command
        Ok(())
    }

    /// Internal: send stop command
    async fn send_stop(&self) -> Result<(), AirPlayError> {
        // TODO: Send RTSP TEARDOWN
        Ok(())
    }

    /// Internal: send generic command
    async fn send_command(&self, command: &str) -> Result<(), AirPlayError> {
        // TODO: Send command via DACP or SET_PARAMETER
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
    pub fn progress(&self) -> f64 {
        if self.duration.is_zero() {
            0.0
        } else {
            self.position.as_secs_f64() / self.duration.as_secs_f64()
        }
    }

    /// Get remaining time
    pub fn remaining(&self) -> Duration {
        self.duration.saturating_sub(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_progress() {
        let progress = PlaybackProgress {
            position: Duration::from_secs(30),
            duration: Duration::from_secs(120),
            rate: 1.0,
        };

        assert_eq!(progress.progress(), 0.25);
        assert_eq!(progress.remaining(), Duration::from_secs(90));
    }

    #[test]
    fn test_progress_zero_duration() {
        let progress = PlaybackProgress {
            position: Duration::from_secs(0),
            duration: Duration::from_secs(0),
            rate: 0.0,
        };

        assert_eq!(progress.progress(), 0.0);
    }
}
```

---

## Acceptance Criteria

- [ ] Play/Pause/Stop commands work
- [ ] Next/Previous track work
- [ ] Seeking works correctly
- [ ] Repeat modes are supported
- [ ] Shuffle modes are supported
- [ ] State is tracked correctly

---

## Notes

- Some devices may not support all controls
- DACP (Digital Audio Control Protocol) may be needed
- Consider adding keyboard/remote control mapping
