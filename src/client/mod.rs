//! Main `AirPlay` client implementation

use crate::connection::{ConnectionManager, ConnectionState};
use crate::control::playback::{PlaybackController, ShuffleMode};
use crate::control::queue::PlaybackQueue;
use crate::control::volume::{Volume, VolumeController};
use crate::discovery::{DiscoveryEvent, discover, scan};
use crate::error::AirPlayError;
use crate::state::{ClientEvent, ClientState, EventBus, StateContainer};
use crate::streaming::{AudioSource, PcmStreamer, UrlStreamer};
use crate::types::{
    AirPlayConfig, AirPlayDevice, PlaybackState, QueueItem, QueueItemId, RepeatMode, TrackInfo,
};

use futures::Stream;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

#[cfg(test)]
mod tests;

/// `AirPlay` client for streaming audio to devices
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::{AirPlayClient, AirPlayConfig};
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// // Create client with default config
/// let client = AirPlayClient::new(AirPlayConfig::default());
///
/// // Discover devices
/// let devices = client.scan(Duration::from_secs(5)).await?;
///
/// if let Some(device) = devices.first() {
///     // Connect to device
///     client.connect(device).await?;
///
///     // Stream audio
///     client.play_url("https://example.com/audio.mp3").await?;
///
///     // Disconnect
///     client.disconnect().await?;
/// }
/// # Ok(())
/// # }
/// ```
pub struct AirPlayClient {
    /// Configuration
    #[allow(dead_code)]
    config: AirPlayConfig,
    /// Connection manager
    connection: Arc<ConnectionManager>,
    /// Playback controller
    playback: Arc<PlaybackController>,
    /// Volume controller
    volume: Arc<VolumeController>,
    /// Playback queue
    queue: Arc<RwLock<PlaybackQueue>>,
    /// PCM streamer
    streamer: Option<Arc<PcmStreamer>>,
    /// URL streamer
    url_streamer: Arc<Mutex<Option<UrlStreamer>>>,
    /// State container
    state: Arc<StateContainer>,
    /// Event bus
    events: Arc<EventBus>,
}

impl AirPlayClient {
    /// Create a new `AirPlay` client
    #[must_use]
    pub fn new(config: AirPlayConfig) -> Self {
        let connection = Arc::new(ConnectionManager::new(config.clone()));
        let playback = Arc::new(PlaybackController::new(connection.clone()));
        let volume = Arc::new(VolumeController::new(connection.clone()));
        let queue = Arc::new(RwLock::new(PlaybackQueue::new()));
        let state = Arc::new(StateContainer::new());
        let events = Arc::new(EventBus::new());
        let url_streamer = Arc::new(Mutex::new(None));

        Self {
            config,
            connection,
            playback,
            volume,
            queue,
            streamer: None,
            url_streamer,
            state,
            events,
        }
    }

    /// Set pairing storage for persistent pairing
    #[must_use]
    pub fn with_pairing_storage(
        mut self,
        storage: Box<dyn crate::protocol::pairing::PairingStorage>,
    ) -> Self {
        // Create new connection manager with storage
        let connection = crate::connection::ConnectionManager::new(self.config.clone())
            .with_pairing_storage(storage);
        let connection = Arc::new(connection);

        // Re-create components that depend on connection
        self.playback = Arc::new(PlaybackController::new(connection.clone()));
        self.volume = Arc::new(VolumeController::new(connection.clone()));
        self.connection = connection;

        self
    }

    /// Create with default configuration
    #[must_use]
    pub fn default_client() -> Self {
        Self::new(AirPlayConfig::default())
    }

    // === Discovery ===

    /// Scan for devices with timeout
    ///
    /// # Errors
    ///
    /// Returns error if mDNS discovery fails.
    pub async fn scan(&self, timeout: Duration) -> Result<Vec<AirPlayDevice>, AirPlayError> {
        scan(timeout).await
    }

    /// Discover devices continuously
    ///
    /// # Errors
    ///
    /// Returns error if mDNS discovery fails.
    pub async fn discover(&self) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
        discover().await
    }

    // === Connection ===

    /// Connect to a device
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        self.connection.connect(device).await?;

        // Update state
        self.state.set_device(Some(device.clone())).await;
        self.events.emit(ClientEvent::Connected {
            device: device.clone(),
        });

        Ok(())
    }

    /// Disconnect from current device
    ///
    /// # Errors
    ///
    /// Returns error if network operation fails.
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        let device = self.state.get().await.device;

        self.connection.disconnect().await?;

        // Update state
        self.state.set_device(None).await;

        if let Some(device) = device {
            self.events.emit(ClientEvent::Disconnected {
                device,
                reason: "User requested".to_string(),
            });
        }

        Ok(())
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.connection.state().await == ConnectionState::Connected
    }

    /// Get connected device
    pub async fn connected_device(&self) -> Option<AirPlayDevice> {
        self.state.get().await.device
    }

    // === Playback ===

    /// Play (resume if paused)
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn play(&self) -> Result<(), AirPlayError> {
        self.playback.play().await?;
        self.state.update(|s| s.playback.is_playing = true).await;
        Ok(())
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.playback.pause().await?;
        self.state.update(|s| s.playback.is_playing = false).await;
        Ok(())
    }

    /// Toggle play/pause
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn toggle_playback(&self) -> Result<(), AirPlayError> {
        self.playback.toggle().await
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.playback.stop().await?;
        self.state
            .update(|s| {
                s.playback.is_playing = false;
                s.playback.position_secs = 0.0;
            })
            .await;
        Ok(())
    }

    /// Skip to next track
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn next(&self) -> Result<(), AirPlayError> {
        self.playback.next().await?;

        // Update queue
        let track = {
            let mut queue = self.queue.write().await;
            queue.next().map(|item| item.track.clone())
        };

        self.state.set_track(track).await;
        Ok(())
    }

    /// Go to previous track
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn previous(&self) -> Result<(), AirPlayError> {
        self.playback.previous().await?;

        let track = {
            let mut queue = self.queue.write().await;
            queue.previous().map(|item| item.track.clone())
        };

        self.state.set_track(track).await;
        Ok(())
    }

    /// Seek to position
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.playback.seek(position).await
    }

    /// Get current playback state
    pub async fn playback_state(&self) -> PlaybackState {
        self.state.get().await.playback
    }

    // === Volume ===

    /// Get current volume
    pub async fn volume(&self) -> f32 {
        self.volume.get().await.as_f32()
    }

    /// Set volume (0.0 - 1.0)
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn set_volume(&self, level: f32) -> Result<(), AirPlayError> {
        self.volume.set(Volume::new(level)).await?;
        self.state.set_volume(level).await;
        self.events
            .emit(ClientEvent::VolumeChanged { volume: level });
        Ok(())
    }

    /// Increase volume
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn volume_up(&self) -> Result<(), AirPlayError> {
        let new_vol = self.volume.step_up().await?;
        self.state.set_volume(new_vol.as_f32()).await;
        Ok(())
    }

    /// Decrease volume
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn volume_down(&self) -> Result<(), AirPlayError> {
        let new_vol = self.volume.step_down().await?;
        self.state.set_volume(new_vol.as_f32()).await;
        Ok(())
    }

    /// Mute
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn mute(&self) -> Result<(), AirPlayError> {
        self.volume.mute().await?;
        self.state.set_muted(true).await;
        Ok(())
    }

    /// Unmute
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn unmute(&self) -> Result<(), AirPlayError> {
        self.volume.unmute().await?;
        self.state.set_muted(false).await;
        Ok(())
    }

    /// Toggle mute
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails.
    pub async fn toggle_mute(&self) -> Result<bool, AirPlayError> {
        let muted = self.volume.toggle_mute().await?;
        self.state.set_muted(muted).await;
        Ok(muted)
    }

    // === Queue ===

    /// Add a track to the queue
    pub async fn add_to_queue(&self, track: TrackInfo) -> QueueItemId {
        let id = self.queue.write().await.add(track);
        self.events.emit(ClientEvent::QueueUpdated {
            length: self.queue.read().await.len(),
        });
        id
    }

    /// Add track to play next
    pub async fn play_next(&self, track: TrackInfo) -> QueueItemId {
        let id = self.queue.write().await.add_next(track);
        self.events.emit(ClientEvent::QueueUpdated {
            length: self.queue.read().await.len(),
        });
        id
    }

    /// Remove from queue
    pub async fn remove_from_queue(&self, id: QueueItemId) {
        self.queue.write().await.remove(id);
        self.events.emit(ClientEvent::QueueUpdated {
            length: self.queue.read().await.len(),
        });
    }

    /// Clear the queue
    pub async fn clear_queue(&self) {
        self.queue.write().await.clear();
        self.events.emit(ClientEvent::QueueUpdated { length: 0 });
    }

    /// Get queue items
    pub async fn queue(&self) -> Vec<QueueItem> {
        self.queue.read().await.items().to_vec()
    }

    /// Enable/disable shuffle
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn set_shuffle(&self, enabled: bool) -> Result<(), AirPlayError> {
        if enabled {
            self.queue.write().await.shuffle();
            self.playback.set_shuffle(ShuffleMode::On).await?;
        } else {
            self.queue.write().await.unshuffle();
            self.playback.set_shuffle(ShuffleMode::Off).await?;
        }
        Ok(())
    }

    /// Set repeat mode
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), AirPlayError> {
        self.playback.set_repeat(mode).await
    }

    // === Streaming ===

    /// Play a URL
    ///
    /// # Errors
    ///
    /// Returns error if playback fails or device is disconnected.
    pub async fn play_url(&self, url: &str) -> Result<(), AirPlayError> {
        if !self.is_connected().await {
            return Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            });
        }

        let mut url_streamer_lock = self.url_streamer.lock().await;

        if url_streamer_lock.is_none() {
            *url_streamer_lock = Some(UrlStreamer::new(self.connection.clone()));
        }

        if let Some(streamer) = url_streamer_lock.as_mut() {
            streamer.play(url).await?;
            self.state.update(|s| s.playback.is_playing = true).await;
        }

        Ok(())
    }

    /// Stream raw PCM audio from a source
    ///
    /// # Errors
    ///
    /// Returns error if streaming fails or device is disconnected.
    pub async fn stream_audio<S: AudioSource + 'static>(
        &mut self,
        source: S,
    ) -> Result<(), AirPlayError> {
        if !self.is_connected().await {
            return Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            });
        }

        let format = source.format();
        let streamer = Arc::new(PcmStreamer::new(self.connection.clone(), format));

        // Configure encryption if available
        if let Some(key) = self.connection.encryption_key().await {
            tracing::debug!("Enabling audio encryption with session key");
            streamer.set_encryption_key(key).await;
        }

        self.streamer = Some(streamer.clone());

        streamer.stream(source).await
    }

    // === Events ===

    /// Subscribe to client events
    #[must_use]
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ClientEvent> {
        self.events.subscribe()
    }

    /// Get current state
    pub async fn state(&self) -> ClientState {
        self.state.get().await
    }

    /// Subscribe to state changes
    #[must_use]
    pub fn subscribe_state(&self) -> tokio::sync::watch::Receiver<ClientState> {
        self.state.subscribe()
    }
}
