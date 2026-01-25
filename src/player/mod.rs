//! High-level player API

use std::time::Duration;

use tokio::sync::RwLock;

use crate::client::AirPlayClient;
use crate::error::AirPlayError;
use crate::types::{AirPlayConfig, AirPlayDevice, PlaybackState, RepeatMode, TrackInfo};

#[cfg(test)]
mod tests;

/// Simplified `AirPlay` player for common use cases
///
/// # Example
///
/// ```rust,no_run
/// use std::time::Duration;
///
/// use airplay2::AirPlayPlayer;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// // Create player and connect to first available device
/// let mut player = AirPlayPlayer::new();
/// player.auto_connect(Duration::from_secs(5)).await?;
///
/// // Play some tracks
/// player
///     .play_tracks(vec![
///         (
///             "http://example.com/1.mp3".to_string(),
///             "Song 1".to_string(),
///             "Artist A".to_string(),
///         ),
///         (
///             "http://example.com/2.mp3".to_string(),
///             "Song 2".to_string(),
///             "Artist B".to_string(),
///         ),
///     ])
///     .await?;
///
/// // Control playback
/// player.pause().await?;
/// player.skip().await?;
/// player.set_volume(0.5).await?;
///
/// # Ok(())
/// # }
/// ```
pub struct AirPlayPlayer {
    /// Underlying client
    client: AirPlayClient,
    /// Auto-reconnect on disconnect
    auto_reconnect: bool,
    /// Target device name for auto-connection
    target_device_name: Option<String>,
    /// Last connected device
    last_device: RwLock<Option<AirPlayDevice>>,
}

impl AirPlayPlayer {
    /// Create a new player with default config
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AirPlayConfig::default())
    }

    /// Create with custom config
    #[must_use]
    pub fn with_config(config: AirPlayConfig) -> Self {
        Self {
            client: AirPlayClient::new(config),
            auto_reconnect: true,
            target_device_name: None,
            last_device: RwLock::new(None),
        }
    }

    /// Enable or disable auto-reconnect
    pub fn set_auto_reconnect(&mut self, enabled: bool) {
        self.auto_reconnect = enabled;
    }

    /// Set target device name for auto-connection
    pub fn set_target_device_name(&mut self, name: Option<String>) {
        self.target_device_name = name;
    }

    // === Quick Connect Methods ===

    /// Auto-connect to first available device (or target device if set)
    ///
    /// # Errors
    ///
    /// Returns error if scanning fails or no suitable device is found.
    pub async fn auto_connect(&self, timeout: Duration) -> Result<AirPlayDevice, AirPlayError> {
        let devices = self.client.scan(timeout).await?;

        let device = if let Some(target_name) = &self.target_device_name {
            let name_lower = target_name.to_lowercase();
            devices
                .into_iter()
                .find(|d| d.name.to_lowercase().contains(&name_lower))
                .ok_or_else(|| AirPlayError::DeviceNotFound {
                    device_id: target_name.clone(),
                })?
        } else {
            devices
                .into_iter()
                .next()
                .ok_or_else(|| AirPlayError::DeviceNotFound {
                    device_id: "any".to_string(),
                })?
        };

        self.connect(&device).await?;
        Ok(device)
    }

    /// Connect to device by name (partial match)
    ///
    /// # Errors
    ///
    /// Returns error if scanning fails or device is not found.
    pub async fn connect_by_name(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<AirPlayDevice, AirPlayError> {
        let devices = self.client.scan(timeout).await?;

        let name_lower = name.to_lowercase();
        let device = devices
            .into_iter()
            .find(|d| d.name.to_lowercase().contains(&name_lower))
            .ok_or_else(|| AirPlayError::DeviceNotFound {
                device_id: name.to_string(),
            })?;

        self.connect(&device).await?;
        Ok(device)
    }

    /// Connect to a specific device
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        self.client.connect(device).await?;
        *self.last_device.write().await = Some(device.clone());
        Ok(())
    }

    /// Disconnect
    ///
    /// # Errors
    ///
    /// Returns error if disconnect fails.
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        self.client.disconnect().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }

    // === Simple Playback ===

    /// Play tracks from a list of (url, title, artist) tuples
    ///
    /// # Errors
    ///
    /// Returns error if adding to queue or playback fails.
    pub async fn play_tracks(
        &self,
        tracks: Vec<(String, String, String)>,
    ) -> Result<(), AirPlayError> {
        self.client.clear_queue().await;

        for (url, title, artist) in &tracks {
            let track = TrackInfo::new(url, title, artist);
            self.client.add_to_queue(track).await;
        }

        // Explicitly start streaming the first track using play_url
        // because client.play() only resumes/starts if state is already primed,
        // and doesn't automatically pull from queue to start streaming yet.
        if let Some((url, _, _)) = tracks.first() {
            self.client.play_url(url).await
        } else {
            // Empty tracks, just try to play/resume whatever state has
            self.client.play().await
        }
    }

    /// Play a single track
    ///
    /// # Errors
    ///
    /// Returns error if adding to queue or playback fails.
    pub async fn play_track(
        &self,
        url: &str,
        title: &str,
        artist: &str,
    ) -> Result<(), AirPlayError> {
        self.play_tracks(vec![(
            url.to_string(),
            title.to_string(),
            artist.to_string(),
        )])
        .await
    }

    /// Resume playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn play(&self) -> Result<(), AirPlayError> {
        self.client.play().await
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.client.pause().await
    }

    /// Toggle play/pause
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn toggle(&self) -> Result<(), AirPlayError> {
        self.client.toggle_playback().await
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.client.stop().await
    }

    /// Skip to next track
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn skip(&self) -> Result<(), AirPlayError> {
        self.client.next().await
    }

    /// Go to previous track
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn back(&self) -> Result<(), AirPlayError> {
        self.client.previous().await
    }

    /// Seek to position (in seconds)
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn seek(&self, seconds: f64) -> Result<(), AirPlayError> {
        self.client.seek(Duration::from_secs_f64(seconds)).await
    }

    // === Volume ===

    /// Set volume (0.0 - 1.0)
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn set_volume(&self, level: f32) -> Result<(), AirPlayError> {
        self.client.set_volume(level).await
    }

    /// Get current volume
    pub async fn volume(&self) -> f32 {
        self.client.volume().await
    }

    /// Mute
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn mute(&self) -> Result<(), AirPlayError> {
        self.client.mute().await
    }

    /// Unmute
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn unmute(&self) -> Result<(), AirPlayError> {
        self.client.unmute().await
    }

    // === Shuffle and Repeat ===

    /// Enable shuffle
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn shuffle_on(&self) -> Result<(), AirPlayError> {
        self.client.set_shuffle(true).await
    }

    /// Disable shuffle
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn shuffle_off(&self) -> Result<(), AirPlayError> {
        self.client.set_shuffle(false).await
    }

    /// Set repeat off
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn repeat_off(&self) -> Result<(), AirPlayError> {
        self.client.set_repeat(RepeatMode::Off).await
    }

    /// Repeat current track
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn repeat_one(&self) -> Result<(), AirPlayError> {
        self.client.set_repeat(RepeatMode::One).await
    }

    /// Repeat all tracks
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn repeat_all(&self) -> Result<(), AirPlayError> {
        self.client.set_repeat(RepeatMode::All).await
    }

    // === Info ===

    /// Get current track info
    pub async fn current_track(&self) -> Option<TrackInfo> {
        self.client.state().await.current_track
    }

    /// Get playback state
    pub async fn playback_state(&self) -> PlaybackState {
        self.client.playback_state().await
    }

    /// Check if playing
    pub async fn is_playing(&self) -> bool {
        self.playback_state().await.is_playing
    }

    /// Get connected device
    pub async fn device(&self) -> Option<AirPlayDevice> {
        self.client.connected_device().await
    }

    /// Get queue length
    pub async fn queue_length(&self) -> usize {
        self.client.queue().await.len()
    }

    // === Advanced ===

    /// Get the underlying client for advanced operations
    #[must_use]
    pub fn client(&self) -> &AirPlayClient {
        &self.client
    }

    /// Get mutable access to underlying client
    pub fn client_mut(&mut self) -> &mut AirPlayClient {
        &mut self.client
    }
}

impl Default for AirPlayPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for `AirPlayPlayer`
pub struct PlayerBuilder {
    config: AirPlayConfig,
    auto_reconnect: bool,
    device_name: Option<String>,
}

impl PlayerBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: AirPlayConfig::default(),
            auto_reconnect: true,
            device_name: None,
        }
    }

    /// Set connection timeout
    #[must_use]
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Set auto-reconnect
    #[must_use]
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set device name filter
    #[must_use]
    pub fn device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = Some(name.into());
        self
    }

    /// Build the player
    #[must_use]
    pub fn build(self) -> AirPlayPlayer {
        let mut player = AirPlayPlayer::with_config(self.config);
        player.auto_reconnect = self.auto_reconnect;
        player.target_device_name = self.device_name;
        player
    }
}

impl Default for PlayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// === Convenience Functions ===

/// Quick play to the first available device
///
/// # Errors
///
/// Returns error if scanning fails, no device found, or playback fails.
pub async fn quick_play(
    tracks: Vec<(String, String, String)>,
) -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.auto_connect(Duration::from_secs(5)).await?;
    player.play_tracks(tracks).await?;
    Ok(player)
}

/// Quick connect and return player
///
/// # Errors
///
/// Returns error if scanning fails or no device found.
pub async fn quick_connect() -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.auto_connect(Duration::from_secs(5)).await?;
    Ok(player)
}

/// Quick connect to named device
///
/// # Errors
///
/// Returns error if scanning fails or device not found.
pub async fn quick_connect_to(name: &str) -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.connect_by_name(name, Duration::from_secs(5)).await?;
    Ok(player)
}
