//! Playback control for `AirPlay`

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::protocol::daap::{DmapProgress, TrackMetadata};
use crate::protocol::plist::DictBuilder;
use crate::protocol::rtsp::Method;
use crate::types::{PlaybackState, RepeatMode};

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

    /// Set playing state
    pub async fn set_playing(&self, playing: bool) {
        self.state.write().await.is_playing = playing;
    }

    /// Play (resume if paused, start if stopped)
    ///
    /// Sends `SetRateAnchorTime` with `rate=1.0` and an RTP/PTP anchor mapping.
    /// When PTP timing is active, includes `networkTimeSecs`, `networkTimeFrac`,
    /// and `networkTimeTimelineID` so the device can map RTP timestamps to its
    /// PTP clock domain and know exactly when to start rendering audio.
    ///
    /// # Errors
    ///
    /// Returns error if state is invalid or network fails
    pub async fn play(&self) -> Result<(), AirPlayError> {
        let mut state = self.state.write().await;

        if !state.is_playing {
            let mut builder = DictBuilder::new()
                .insert("rate", 1i64)
                .insert("rtpTime", 0u64);

            // Include PTP anchor timestamps so the device knows when to render
            if let Some((secs, frac, timeline_id)) =
                self.connection.get_ptp_network_time().await
            {
                tracing::info!(
                    "SetRateAnchorTime: anchoring rtpTime=0 to PTP time {}.{:09} (timeline=0x{:016X})",
                    secs,
                    // Convert frac back to nanos for display: nanos = frac * 10^9 / 2^64
                    ((frac as u128 * 1_000_000_000u128) >> 64) as u32,
                    timeline_id,
                );
                builder = builder
                    .insert("networkTimeSecs", secs)
                    .insert("networkTimeFrac", frac)
                    .insert("networkTimeTimelineID", timeline_id);
            } else {
                tracing::warn!(
                    "SetRateAnchorTime: PTP clock not available or not synchronized — \
                     sending without networkTime fields (device may not render audio)"
                );
            }

            let body = builder.build();
            let encoded =
                crate::protocol::plist::encode(&body).map_err(|e| AirPlayError::RtspError {
                    message: format!("Failed to encode plist: {e}"),
                    status_code: None,
                })?;

            self.connection
                .send_command(
                    Method::SetRateAnchorTime,
                    Some(encoded),
                    Some("application/x-apple-binary-plist".to_string()),
                )
                .await?;
            state.is_playing = true;
        }

        Ok(())
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        let mut state = self.state.write().await;

        if state.is_playing {
            let body = DictBuilder::new()
                .insert("rate", 0i64)
                .insert("rtpTime", 0u64)
                .build();
            let encoded =
                crate::protocol::plist::encode(&body).map_err(|e| AirPlayError::RtspError {
                    message: format!("Failed to encode plist: {e}"),
                    status_code: None,
                })?;

            self.connection
                .send_command(
                    Method::SetRateAnchorTime,
                    Some(encoded),
                    Some("application/x-apple-binary-plist".to_string()),
                )
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

    /// Set track metadata
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_metadata(&self, metadata: TrackMetadata) -> Result<(), AirPlayError> {
        self.send_metadata(metadata.clone()).await?;
        let mut state = self.state.write().await;
        state.current_track = metadata.title.clone().map(|t| crate::types::TrackInfo {
            url: String::new(),
            title: t,
            artist: metadata.artist.clone().unwrap_or_default(),
            album: metadata.album.clone(),
            artwork_url: None,
            duration_secs: metadata.duration_ms.map(|d| f64::from(d) / 1000.0),
            track_number: metadata.track_number,
            disc_number: metadata.disc_number,
            genre: metadata.genre,
            content_id: None,
        });
        Ok(())
    }

    /// Set playback progress
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_progress(&self, progress: DmapProgress) -> Result<(), AirPlayError> {
        let body = progress.encode();
        self.connection
            .send_command(
                Method::SetParameter,
                Some(body.into_bytes()),
                Some("text/parameters".to_string()),
            )
            .await?;
        Ok(())
    }

    /// Internal: send scrub command
    async fn send_scrub(&self, position: f64) -> Result<(), AirPlayError> {
        // AirPlay 2 uses progress parameter for scrub
        // We need a base RTP timestamp. For now we use a dummy one if not provided,
        // but high-quality implementations should track the actual RTP base.
        let base_rtp: u32 = 0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pos_samples = (position * 44100.0) as u32;
        // We don't know duration here, so we use current for end as well or a large value
        let progress = DmapProgress::new(
            base_rtp,
            base_rtp.wrapping_add(pos_samples),
            base_rtp.wrapping_add(pos_samples),
        );

        self.set_progress(progress).await
    }

    /// Internal: send metadata command
    async fn send_metadata(&self, metadata: TrackMetadata) -> Result<(), AirPlayError> {
        let body = metadata.encode_dmap();
        self.connection
            .send_command(
                Method::SetParameter,
                Some(body),
                Some("application/x-dmap-tagged".to_string()),
            )
            .await?;
        Ok(())
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

/// Playback progress information for state reporting
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
