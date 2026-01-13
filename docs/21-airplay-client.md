# Section 21: AirPlayClient Implementation

## Dependencies
- **Section 08**: mDNS Discovery (must be complete)
- **Section 10**: Connection Management (must be complete)
- **Section 13**: PCM Streaming (must be complete)
- **Section 15**: Playback Control (must be complete)
- **Section 17**: State and Events (must be complete)
- **Section 18**: Volume Control (must be complete)

## Overview

This is the main client implementation that ties together all components into a cohesive API. The `AirPlayClient` provides:
- Device discovery
- Connection management
- Audio streaming
- Playback control
- Event subscription

## Objectives

- Create unified client interface
- Manage component lifecycle
- Provide ergonomic API
- Handle errors gracefully

---

## Tasks

### 21.1 AirPlayClient

- [ ] **21.1.1** Implement the main client

**File:** `src/client.rs`

```rust
//! Main AirPlay client implementation

use crate::discovery::{discover, scan, DiscoveryEvent};
use crate::connection::{ConnectionManager, ConnectionState, ConnectionEvent};
use crate::streaming::{AudioSource, PcmStreamer};
use crate::control::playback::{PlaybackController, RepeatMode, ShuffleMode};
use crate::control::queue::{PlaybackQueue, QueueItem, QueueItemId};
use crate::control::volume::{Volume, VolumeController};
use crate::state::{ClientState, StateContainer, EventBus, ClientEvent};
use crate::types::{AirPlayDevice, AirPlayConfig, TrackInfo, PlaybackState};
use crate::audio::AudioFormat;
use crate::error::AirPlayError;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use futures::Stream;

/// AirPlay client for streaming audio to devices
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
///     // client.play_url("https://example.com/audio.mp3").await?;
///
///     // Disconnect
///     client.disconnect().await?;
/// }
/// # Ok(())
/// # }
/// ```
pub struct AirPlayClient {
    /// Configuration
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
    /// State container
    state: Arc<StateContainer>,
    /// Event bus
    events: Arc<EventBus>,
}

impl AirPlayClient {
    /// Create a new AirPlay client
    pub fn new(config: AirPlayConfig) -> Self {
        let connection = Arc::new(ConnectionManager::new(config.clone()));
        let playback = Arc::new(PlaybackController::new(connection.clone()));
        let volume = Arc::new(VolumeController::new(connection.clone()));
        let queue = Arc::new(RwLock::new(PlaybackQueue::new()));
        let state = Arc::new(StateContainer::new());
        let events = Arc::new(EventBus::new());

        Self {
            config,
            connection,
            playback,
            volume,
            queue,
            streamer: None,
            state,
            events,
        }
    }

    /// Create with default configuration
    pub fn default_client() -> Self {
        Self::new(AirPlayConfig::default())
    }

    // === Discovery ===

    /// Scan for devices with timeout
    pub async fn scan(&self, timeout: Duration) -> Result<Vec<AirPlayDevice>, AirPlayError> {
        scan(timeout).await
    }

    /// Discover devices continuously
    pub async fn discover(&self) -> impl Stream<Item = DiscoveryEvent> {
        discover().await
    }

    // === Connection ===

    /// Connect to a device
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
    pub async fn play(&self) -> Result<(), AirPlayError> {
        self.playback.play().await?;
        self.state.set_playback(PlaybackState::Playing).await;
        Ok(())
    }

    /// Pause playback
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.playback.pause().await?;
        self.state.set_playback(PlaybackState::Paused).await;
        Ok(())
    }

    /// Toggle play/pause
    pub async fn toggle_playback(&self) -> Result<(), AirPlayError> {
        self.playback.toggle().await
    }

    /// Stop playback
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.playback.stop().await?;
        self.state.set_playback(PlaybackState::Stopped).await;
        Ok(())
    }

    /// Skip to next track
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
    pub async fn set_volume(&self, level: f32) -> Result<(), AirPlayError> {
        self.volume.set(Volume::new(level)).await?;
        self.state.set_volume(level).await;
        self.events.emit(ClientEvent::VolumeChanged { volume: level });
        Ok(())
    }

    /// Increase volume
    pub async fn volume_up(&self) -> Result<(), AirPlayError> {
        let new_vol = self.volume.step_up().await?;
        self.state.set_volume(new_vol.as_f32()).await;
        Ok(())
    }

    /// Decrease volume
    pub async fn volume_down(&self) -> Result<(), AirPlayError> {
        let new_vol = self.volume.step_down().await?;
        self.state.set_volume(new_vol.as_f32()).await;
        Ok(())
    }

    /// Mute
    pub async fn mute(&self) -> Result<(), AirPlayError> {
        self.volume.mute().await?;
        self.state.set_muted(true).await;
        Ok(())
    }

    /// Unmute
    pub async fn unmute(&self) -> Result<(), AirPlayError> {
        self.volume.unmute().await?;
        self.state.set_muted(false).await;
        Ok(())
    }

    /// Toggle mute
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
    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), AirPlayError> {
        self.playback.set_repeat(mode).await
    }

    // === Streaming ===

    /// Stream raw PCM audio from a source
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
        self.streamer = Some(streamer.clone());

        streamer.stream(source).await
    }

    // === Events ===

    /// Subscribe to client events
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ClientEvent> {
        self.events.subscribe()
    }

    /// Get current state
    pub async fn state(&self) -> ClientState {
        self.state.get().await
    }

    /// Subscribe to state changes
    pub fn subscribe_state(&self) -> tokio::sync::watch::Receiver<ClientState> {
        self.state.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = AirPlayClient::default_client();
        assert!(!client.is_connected().await);
    }

    #[tokio::test]
    async fn test_queue_operations() {
        let client = AirPlayClient::default_client();

        let track = TrackInfo {
            title: Some("Test Track".to_string()),
            artist: Some("Test Artist".to_string()),
            album: None,
            duration: Some(Duration::from_secs(180)),
            artwork_url: None,
        };

        let id = client.add_to_queue(track.clone()).await;
        let queue = client.queue().await;

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].track.title, track.title);

        client.remove_from_queue(id).await;
        assert!(client.queue().await.is_empty());
    }
}
```

---

## Acceptance Criteria

- [ ] Client can be created with config
- [ ] Discovery methods work
- [ ] Connection lifecycle is managed
- [ ] Playback controls work
- [ ] Volume controls work
- [ ] Queue management works
- [ ] Events are emitted
- [ ] All unit tests pass

---

## Notes

- Client is the main entry point for users
- All methods should be async
- Consider adding builder pattern
- May need cleanup on drop
