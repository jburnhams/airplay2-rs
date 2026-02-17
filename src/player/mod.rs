//! High-level player API

use crate::client::AirPlayClient;
use crate::error::AirPlayError;
use crate::state::ClientEvent;
use crate::types::{AirPlayConfig, AirPlayDevice, PlaybackState, RepeatMode, TrackInfo};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[cfg(test)]
mod tests;

/// Simplified `AirPlay` player for common use cases
#[derive(Clone)]
pub struct AirPlayPlayer {
    /// Underlying client
    client: AirPlayClient,
    /// Auto-reconnect on disconnect
    auto_reconnect: Arc<AtomicBool>,
    /// Target device name for auto-connection
    target_device_name: Arc<RwLock<Option<String>>>,
    /// Last connected device
    last_device: Arc<RwLock<Option<AirPlayDevice>>>,
    /// Reconnection in progress flag
    is_reconnecting: Arc<AtomicBool>,
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
        let player = Self {
            client: AirPlayClient::new(config),
            auto_reconnect: Arc::new(AtomicBool::new(true)),
            target_device_name: Arc::new(RwLock::new(None)),
            last_device: Arc::new(RwLock::new(None)),
            is_reconnecting: Arc::new(AtomicBool::new(false)),
        };

        player.start_reconnect_monitor();
        player
    }

    /// Enable or disable auto-reconnect
    pub fn set_auto_reconnect(&mut self, enabled: bool) {
        self.auto_reconnect.store(enabled, Ordering::SeqCst);
    }

    /// Set target device name for auto-connection
    pub async fn set_target_device_name(&self, name: Option<String>) {
        *self.target_device_name.write().await = name;
    }

    fn start_reconnect_monitor(&self) {
        let client = self.client.clone();
        let auto_reconnect = self.auto_reconnect.clone();
        let last_device = self.last_device.clone();
        let is_reconnecting = self.is_reconnecting.clone();
        let mut events = client.subscribe_events();

        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let ClientEvent::Disconnected { reason, .. } = event {
                    tracing::info!("Player detected disconnect: {}", reason);

                    // Check if we should reconnect
                    // Don't reconnect if user requested it explicitly via disconnect()
                    // (which typically sends UserRequested reason)
                    let should_reconnect = auto_reconnect.load(Ordering::SeqCst);
                    if !should_reconnect || reason.contains("UserRequested") {
                        tracing::info!("Ignoring disconnect event (Auto-reconnect disabled or UserRequested).");
                        continue;
                    }

                    if is_reconnecting.swap(true, Ordering::SeqCst) {
                        tracing::debug!("Reconnection already in progress");
                        continue;
                    }

                    tracing::info!("Attempting auto-reconnect in 2s...");
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // Reconnection loop
                    let mut attempts = 0;
                    let max_attempts = 10;
                    let mut success = false;

                    while attempts < max_attempts && auto_reconnect.load(Ordering::SeqCst) {
                        attempts += 1;
                        tracing::info!("Reconnection attempt {}/{}", attempts, max_attempts);

                        // Try to get last device info
                        let target_device = last_device.read().await.clone();

                        if let Some(mut device) = target_device {
                            // 1. Try connecting directly (fast path)
                            // This works if IP/Port hasn't changed
                            if let Err(e) = client.connect(&device).await {
                                tracing::debug!("Direct connection failed: {}. Retrying with scan...", e);

                                // 2. If direct fails, try to re-discover device (IP/Port might have changed)
                                // We scan for a device with the same ID
                                match client.scan(Duration::from_secs(3)).await {
                                    Ok(devices) => {
                                        if let Some(updated_device) = devices.into_iter().find(|d| d.id == device.id) {
                                            tracing::info!("Found updated device info: {:?} -> {:?}", device.addresses, updated_device.addresses);
                                            device = updated_device;
                                            // Update last_device with new info
                                            *last_device.write().await = Some(device.clone());

                                            if let Err(e) = client.connect(&device).await {
                                                 tracing::warn!("Reconnection failed after scan: {}", e);
                                            } else {
                                                success = true;
                                                break;
                                            }
                                        } else {
                                            tracing::warn!("Device {} not found in scan", device.id);
                                        }
                                    }
                                    Err(e) => tracing::warn!("Scan failed during reconnect: {}", e),
                                }
                            } else {
                                success = true;
                                break;
                            }
                        } else {
                            tracing::warn!("No last device to reconnect to");
                            break;
                        }

                        // Exponential backoff
                        let backoff = Duration::from_secs(2u64.pow(attempts.min(4) as u32));
                        tokio::time::sleep(backoff).await;
                    }

                    if success {
                        tracing::info!("✓ Auto-reconnected successfully");
                    } else {
                        tracing::error!("Failed to auto-reconnect after {} attempts", max_attempts);
                    }

                    is_reconnecting.store(false, Ordering::SeqCst);
                }
            }
        });
    }

    // === Quick Connect Methods ===

    /// Auto-connect to first available device (or target device if set)
    ///
    /// # Errors
    ///
    /// Returns error if scanning fails or no suitable device is found.
    pub async fn auto_connect(&self, timeout: Duration) -> Result<AirPlayDevice, AirPlayError> {
        let devices = self.client.scan(timeout).await?;

        let target = self.target_device_name.read().await.clone();

        let device = if let Some(target_name) = target {
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

        if let Some((url, _, _)) = tracks.first() {
            self.client.play_url(url).await
        } else {
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

    /// Play a local file (requires `decoders` feature)
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be opened or playback fails.
    #[cfg(feature = "decoders")]
    pub async fn play_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), AirPlayError> {
        let source =
            crate::streaming::file::FileSource::new(path).map_err(|e| AirPlayError::IoError {
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;

        // Ensure connected
        if !self.is_connected().await {
             if let Some(ref name) = *self.target_device_name.read().await {
                self.connect_by_name(name, Duration::from_secs(5)).await?;
            } else {
                let last = self.last_device.read().await.clone();
                if let Some(d) = last {
                    self.connect(&d).await?;
                } else {
                    return Err(AirPlayError::InvalidState {
                        message: "Not connected to any device".to_string(),
                        current_state: "Disconnected".to_string(),
                    });
                }
            }
        }

        self.client.stream_audio(source).await
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
        player.set_auto_reconnect(self.auto_reconnect);
        if let Some(name) = self.device_name {
            player.target_device_name = Arc::new(RwLock::new(Some(name)));
        }
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
pub async fn quick_play(tracks: Vec<(String, String, String)>) -> Result<AirPlayPlayer, AirPlayError> {
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
