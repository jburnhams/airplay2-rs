//! Main `AirPlay` client implementation

use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use tokio::sync::{Mutex, RwLock};

use crate::audio::AudioCodec;
use crate::connection::{ConnectionManager, ConnectionState, DisconnectReason};
use crate::control::playback::{PlaybackController, ShuffleMode};
use crate::control::queue::PlaybackQueue;
use crate::control::volume::{Volume, VolumeController};
use crate::discovery::{DiscoveryEvent, discover, scan};
use crate::error::AirPlayError;
use crate::protocol::daap::{DmapProgress, TrackMetadata};
use crate::state::{ClientEvent, ClientState, EventBus, StateContainer};
use crate::streaming::{AudioSource, PcmStreamer, UrlStreamer};
use crate::types::{
    AirPlayConfig, AirPlayDevice, PlaybackState, QueueItem, QueueItemId, RepeatMode, TrackInfo,
};

pub mod protocol;
pub mod session;

#[cfg(test)]
mod tests;

pub use protocol::{PreferredProtocol, SelectedProtocol, check_raop_encryption, select_protocol};
pub use session::{AirPlay2SessionImpl, AirPlaySession, RaopSessionImpl};

/// `AirPlay` client for streaming audio to devices
///
/// # Example
///
/// ```rust,no_run
/// use std::time::Duration;
///
/// use airplay2::{AirPlayClient, AirPlayConfig};
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
#[derive(Clone)]
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
    pub fn discover(&self) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
        discover()
    }

    // === Connection ===

    /// Connect to a device
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        self.connection.connect(device).await?;

        // Start background tasks
        self.start_monitor();
        self.start_keep_alive();

        // Update state
        self.state.set_device(Some(device.clone())).await;
        self.events.emit(ClientEvent::Connected {
            device: device.clone(),
        });

        Ok(())
    }

    /// Forget a paired device
    ///
    /// Removes persistent pairing keys for the specified device ID.
    ///
    /// # Errors
    ///
    /// Returns error if storage operation fails.
    pub async fn forget_device(&self, device_id: &str) -> Result<(), AirPlayError> {
        self.connection.remove_pairing(device_id).await
    }

    fn start_monitor(&self) {
        let connection = self.connection.clone();
        let events = self.events.clone();
        let state = self.state.clone();
        let mut rx = connection.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                use crate::connection::ConnectionEvent;
                match event {
                    ConnectionEvent::Disconnected { device, reason } => {
                        state.set_device(None).await;
                        events.emit(ClientEvent::Disconnected {
                            device,
                            reason: format!("{reason:?}"),
                        });
                        // Stop monitor loop on disconnect
                        break;
                    }
                    ConnectionEvent::Connected { device } => {
                        state.set_device(Some(device.clone())).await;
                        events.emit(ClientEvent::Connected { device });
                    }
                    ConnectionEvent::Error {
                        message,
                        recoverable,
                    } => {
                        // Forward errors?
                        tracing::warn!(
                            "Connection error: {} (recoverable: {})",
                            message,
                            recoverable
                        );
                    }
                    _ => {}
                }
            }
        });
    }

    fn start_keep_alive(&self) {
        let connection = self.connection.clone();

        tokio::spawn(async move {
            // Check more frequently (1s) to detect disconnection faster
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                // Check if connected
                let state = connection.state().await;
                if state != ConnectionState::Connected {
                    if state == ConnectionState::Disconnected {
                        tracing::debug!("Keep-alive stopping (disconnected)");
                        break;
                    }
                    continue;
                }

                tracing::debug!("Sending keep-alive (GET /info)");
                // Send keep-alive (GET /info)
                match connection.send_get_command("/info").await {
                    Ok(_) => {
                        tracing::debug!("Keep-alive success");
                    }
                    Err(e) => {
                        tracing::warn!("Keep-alive failed: {}", e);
                        // Trigger disconnect
                        let reason = DisconnectReason::NetworkError(e.to_string());

                        // We must force state update because send_get_command might not have done
                        // it if it failed at logic level. But if it failed
                        // with IO error, connection might be broken.
                        // Calling disconnect_with_reason will ensure state update and event
                        // emission.
                        let _ = connection.disconnect_with_reason(reason).await;
                        break;
                    }
                }
            }
        });
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
                reason: "UserRequested".to_string(),
            });
        }

        Ok(())
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.connection.state().await == ConnectionState::Connected
    }

    /// Helper to ensure client is connected
    ///
    /// # Errors
    ///
    /// Returns `AirPlayError::Disconnected` if not connected
    async fn ensure_connected(&self) -> Result<(), AirPlayError> {
        if !self.is_connected().await {
            let device_name = self
                .connection
                .device()
                .await
                .map_or_else(|| "none".to_string(), |d| d.name);
            return Err(AirPlayError::Disconnected { device_name });
        }
        Ok(())
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
        self.playback.toggle().await
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails.
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
        self.playback.next().await?;

        // Update queue
        let track = {
            let mut queue = self.queue.write().await;
            queue.advance().map(|item| item.track.clone())
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
        let muted = self.volume.toggle_mute().await?;
        self.state.set_muted(muted).await;
        Ok(muted)
    }

    /// Set track metadata
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_metadata(&self, metadata: TrackMetadata) -> Result<(), AirPlayError> {
        self.playback.set_metadata(metadata).await
    }

    /// Set playback progress
    ///
    /// # Errors
    ///
    /// Returns error if network fails
    pub async fn set_progress(&self, progress: DmapProgress) -> Result<(), AirPlayError> {
        self.playback.set_progress(progress).await
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
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
        self.playback.set_repeat(mode).await
    }

    // === Streaming ===

    /// Play a URL
    ///
    /// # Errors
    ///
    /// Returns error if playback fails or device is disconnected.
    pub async fn play_url(&self, url: &str) -> Result<(), AirPlayError> {
        self.ensure_connected().await?;

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

    /// Get playback info from device (debug)
    ///
    /// Sends a `GET_PARAMETER` request with body "playback-info\r\n"
    ///
    /// # Errors
    ///
    /// Returns error if network operation fails.
    pub async fn get_playback_info(&self) -> Result<Vec<u8>, AirPlayError> {
        self.ensure_connected().await?;

        // Try querying playback-info
        // Some devices might prefer "progress"
        let body = "playback-info\r\n";
        self.connection
            .send_command(
                crate::protocol::rtsp::Method::GetParameter,
                Some(body.as_bytes().to_vec()),
                Some("text/parameters".to_string()),
            )
            .await
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
        self.ensure_connected().await?;

        // AirPlay 2 typically uses 44.1kHz, 16-bit, Stereo.
        // We configure the streamer with this target format so that it can
        // automatically resample/convert the source if needed.
        let target_format = crate::audio::AudioFormat {
            sample_rate: crate::audio::SampleRate::Hz44100,
            channels: crate::audio::ChannelConfig::Stereo,
            sample_format: crate::audio::SampleFormat::I16,
        };

        let streamer = Arc::new(PcmStreamer::new(
            self.connection.clone(),
            target_format,
            self.config.audio_buffer_frames,
        ));

        // Enable ALAC encoding if configured
        if self.config.audio_codec == AudioCodec::Alac {
            streamer.use_alac().await;
        } else if self.config.audio_codec == AudioCodec::Aac {
            streamer.use_aac(self.config.aac_bitrate).await;
        } else if self.config.audio_codec == AudioCodec::AacEld {
            streamer.use_aac_eld(self.config.aac_bitrate).await;
        }

        // Configure encryption if available
        if let Some(key) = self.connection.encryption_key().await {
            tracing::debug!("Enabling audio encryption with session key");
            streamer.set_encryption_key(key).await;
        }

        self.streamer = Some(streamer.clone());

        self.state.update(|s| s.playback.is_playing = true).await;
        self.playback.set_playing(true).await;

        // Send SETRATEANCHORTIME after audio starts flowing.
        // RECORD was already sent during connect(). SETRATEANCHORTIME tells the device
        // to start rendering audio at the given rate and PTP-anchored time.
        let connection = self.connection.clone();
        tokio::spawn(async move {
            // Wait for audio data to fill the device buffer
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Retry SETRATEANCHORTIME with increasing delays
            let delays_ms = [1000, 2000, 3000, 5000, 8000];
            for (attempt, delay) in delays_ms.iter().enumerate() {
                tracing::info!("Sending SETRATEANCHORTIME attempt {}/{}...", attempt + 1, delays_ms.len());
                match connection.send_set_rate_anchor_time(1).await {
                    Ok(()) => {
                        tracing::info!("SETRATEANCHORTIME succeeded on attempt {}", attempt + 1);
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("SETRATEANCHORTIME attempt {} failed: {}", attempt + 1, e);
                        tokio::time::sleep(Duration::from_millis(*delay)).await;
                    }
                }
            }
            tracing::error!("SETRATEANCHORTIME failed after all retries");
        });

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

    /// Check if PTP timing is active for the current connection.
    pub async fn is_ptp_active(&self) -> bool {
        self.connection.is_ptp_active().await
    }

    /// Get the shared PTP clock, if PTP timing is active.
    pub async fn ptp_clock(
        &self,
    ) -> Option<crate::protocol::ptp::handler::SharedPtpClock> {
        self.connection.ptp_clock().await
    }
}

/// Unified `AirPlay` client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Preferred protocol
    pub preferred_protocol: PreferredProtocol,
    /// Connection timeout
    pub connection_timeout: std::time::Duration,
    /// Enable DACP remote control (RAOP)
    pub enable_dacp: bool,
    /// Enable metadata transmission
    pub enable_metadata: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            preferred_protocol: PreferredProtocol::PreferAirPlay2,
            connection_timeout: std::time::Duration::from_secs(10),
            enable_dacp: true,
            enable_metadata: true,
        }
    }
}

/// Unified `AirPlay` client that supports both `AirPlay` 2 and `RAOP` protocols.
///
/// This client automatically selects the best available protocol for a device
/// (preferring `AirPlay` 2 by default) and provides a common interface for
/// connection, playback control, and streaming.
pub struct UnifiedAirPlayClient {
    /// Client configuration determining protocol selection and behavior
    config: ClientConfig,
    /// Currently active session (either `AirPlay` 2 or `RAOP`)
    session: Option<Box<dyn AirPlaySession>>,
    /// Information about the currently connected device
    device: Option<AirPlayDevice>,
    /// The protocol currently being used
    protocol: Option<SelectedProtocol>,
}

impl UnifiedAirPlayClient {
    /// Create new client with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    /// Create client with custom configuration
    #[must_use]
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            config,
            session: None,
            device: None,
            protocol: None,
        }
    }

    /// Connect to a discovered device
    ///
    /// # Errors
    ///
    /// Returns error if connection fails or no protocol can be selected.
    pub async fn connect(&mut self, device: AirPlayDevice) -> Result<(), AirPlayError> {
        // Select protocol
        let protocol = select_protocol(&device, self.config.preferred_protocol).map_err(|e| {
            AirPlayError::ConnectionFailed {
                device_name: device.name.clone(),
                message: e.to_string(),
                source: None,
            }
        })?;

        // Create appropriate session
        let mut session: Box<dyn AirPlaySession> = match protocol {
            SelectedProtocol::AirPlay2 => Box::new(AirPlay2SessionImpl::new(
                device.clone(),
                AirPlayConfig::default(),
            )),
            SelectedProtocol::Raop => {
                let addr = device.address();
                let port = device.raop_port.unwrap_or(5000);
                Box::new(RaopSessionImpl::new(&addr.to_string(), port))
            }
        };

        // Connect
        session.connect().await?;

        self.session = Some(session);
        self.device = Some(device);
        self.protocol = Some(protocol);

        Ok(())
    }

    /// Disconnect from current device
    ///
    /// # Errors
    ///
    /// Returns error if disconnection fails.
    pub async fn disconnect(&mut self) -> Result<(), AirPlayError> {
        if let Some(ref mut session) = self.session {
            session.disconnect().await?;
        }
        self.session = None;
        self.device = None;
        self.protocol = None;
        Ok(())
    }

    /// Check if connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.is_connected())
    }

    /// Get selected protocol
    #[must_use]
    pub fn protocol(&self) -> Option<SelectedProtocol> {
        self.protocol
    }

    /// Get session reference
    #[must_use]
    pub fn session(&self) -> Option<&dyn AirPlaySession> {
        self.session.as_deref()
    }

    /// Get mutable session reference
    #[must_use]
    pub fn session_mut(&mut self) -> Option<&mut (dyn AirPlaySession + 'static)> {
        self.session.as_deref_mut()
    }

    /// Start playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails or not connected.
    pub async fn play(&mut self) -> Result<(), AirPlayError> {
        if let Some(session) = self.session_mut() {
            session.play().await
        } else {
            Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            })
        }
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails or not connected.
    pub async fn pause(&mut self) -> Result<(), AirPlayError> {
        if let Some(session) = self.session_mut() {
            session.pause().await
        } else {
            Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            })
        }
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if playback command fails or not connected.
    pub async fn stop(&mut self) -> Result<(), AirPlayError> {
        if let Some(session) = self.session_mut() {
            session.stop().await
        } else {
            Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            })
        }
    }

    /// Set volume
    ///
    /// # Errors
    ///
    /// Returns error if volume command fails or not connected.
    pub async fn set_volume(&mut self, volume: f32) -> Result<(), AirPlayError> {
        if let Some(session) = self.session_mut() {
            session.set_volume(volume).await
        } else {
            Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            })
        }
    }

    /// Stream audio data
    ///
    /// # Errors
    ///
    /// Returns error if streaming fails or not connected.
    pub async fn stream_audio(&mut self, data: &[u8]) -> Result<(), AirPlayError> {
        if let Some(session) = self.session_mut() {
            session.stream_audio(data).await
        } else {
            Err(AirPlayError::Disconnected {
                device_name: "none".to_string(),
            })
        }
    }
}

impl Default for UnifiedAirPlayClient {
    fn default() -> Self {
        Self::new()
    }
}
