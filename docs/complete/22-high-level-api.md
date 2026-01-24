# Section 22: High-Level API (AirPlayPlayer)

## Dependencies
- **Section 21**: AirPlayClient (must be complete)

## Overview

A simplified high-level API wrapper for common use cases. While `AirPlayClient` provides full control, `AirPlayPlayer` offers a streamlined interface for typical music player scenarios.

## Objectives

- Provide simple, intuitive API
- Handle common patterns automatically
- Reduce boilerplate for typical use cases
- Maintain flexibility for advanced users

---

## Tasks

### 22.1 AirPlayPlayer

- [x] **22.1.1** Implement high-level player

**File:** `src/player.rs`

```rust
//! High-level player API

use crate::client::AirPlayClient;
use crate::types::{AirPlayDevice, AirPlayConfig, TrackInfo, PlaybackState};
use crate::control::playback::RepeatMode;
use crate::error::AirPlayError;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Simplified AirPlay player for common use cases
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::AirPlayPlayer;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// // Create player and connect to first available device
/// let mut player = AirPlayPlayer::new();
/// player.auto_connect(Duration::from_secs(5)).await?;
///
/// // Play some tracks
/// player.play_tracks(vec![
///     ("Song 1".to_string(), "Artist A".to_string()),
///     ("Song 2".to_string(), "Artist B".to_string()),
/// ]).await?;
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
    /// Last connected device
    last_device: RwLock<Option<AirPlayDevice>>,
}

impl AirPlayPlayer {
    /// Create a new player with default config
    pub fn new() -> Self {
        Self::with_config(AirPlayConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: AirPlayConfig) -> Self {
        Self {
            client: AirPlayClient::new(config),
            auto_reconnect: true,
            last_device: RwLock::new(None),
        }
    }

    /// Enable or disable auto-reconnect
    pub fn set_auto_reconnect(&mut self, enabled: bool) {
        self.auto_reconnect = enabled;
    }

    // === Quick Connect Methods ===

    /// Auto-connect to first available device
    pub async fn auto_connect(&self, timeout: Duration) -> Result<AirPlayDevice, AirPlayError> {
        let devices = self.client.scan(timeout).await?;

        let device = devices.into_iter().next().ok_or(AirPlayError::DeviceNotFound {
            device_id: "any".to_string(),
        })?;

        self.connect(&device).await?;
        Ok(device)
    }

    /// Connect to device by name (partial match)
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
            .ok_or(AirPlayError::DeviceNotFound {
                device_id: name.to_string(),
            })?;

        self.connect(&device).await?;
        Ok(device)
    }

    /// Connect to a specific device
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        self.client.connect(device).await?;
        *self.last_device.write().await = Some(device.clone());
        Ok(())
    }

    /// Disconnect
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        self.client.disconnect().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }

    // === Simple Playback ===

    /// Play tracks from a list of (title, artist) tuples
    pub async fn play_tracks(
        &self,
        tracks: Vec<(String, String)>,
    ) -> Result<(), AirPlayError> {
        self.client.clear_queue().await;

        for (title, artist) in tracks {
            let track = TrackInfo {
                title: Some(title),
                artist: Some(artist),
                album: None,
                duration: None,
                artwork_url: None,
            };
            self.client.add_to_queue(track).await;
        }

        self.client.play().await
    }

    /// Play a single track
    pub async fn play_track(
        &self,
        title: &str,
        artist: &str,
    ) -> Result<(), AirPlayError> {
        self.play_tracks(vec![(title.to_string(), artist.to_string())]).await
    }

    /// Resume playback
    pub async fn play(&self) -> Result<(), AirPlayError> {
        self.client.play().await
    }

    /// Pause playback
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.client.pause().await
    }

    /// Toggle play/pause
    pub async fn toggle(&self) -> Result<(), AirPlayError> {
        self.client.toggle_playback().await
    }

    /// Stop playback
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.client.stop().await
    }

    /// Skip to next track
    pub async fn skip(&self) -> Result<(), AirPlayError> {
        self.client.next().await
    }

    /// Go to previous track
    pub async fn back(&self) -> Result<(), AirPlayError> {
        self.client.previous().await
    }

    /// Seek to position (in seconds)
    pub async fn seek(&self, seconds: f64) -> Result<(), AirPlayError> {
        self.client.seek(Duration::from_secs_f64(seconds)).await
    }

    // === Volume ===

    /// Set volume (0.0 - 1.0)
    pub async fn set_volume(&self, level: f32) -> Result<(), AirPlayError> {
        self.client.set_volume(level).await
    }

    /// Get current volume
    pub async fn volume(&self) -> f32 {
        self.client.volume().await
    }

    /// Mute
    pub async fn mute(&self) -> Result<(), AirPlayError> {
        self.client.mute().await
    }

    /// Unmute
    pub async fn unmute(&self) -> Result<(), AirPlayError> {
        self.client.unmute().await
    }

    // === Shuffle and Repeat ===

    /// Enable shuffle
    pub async fn shuffle_on(&self) -> Result<(), AirPlayError> {
        self.client.set_shuffle(true).await
    }

    /// Disable shuffle
    pub async fn shuffle_off(&self) -> Result<(), AirPlayError> {
        self.client.set_shuffle(false).await
    }

    /// Set repeat off
    pub async fn repeat_off(&self) -> Result<(), AirPlayError> {
        self.client.set_repeat(RepeatMode::Off).await
    }

    /// Repeat current track
    pub async fn repeat_one(&self) -> Result<(), AirPlayError> {
        self.client.set_repeat(RepeatMode::One).await
    }

    /// Repeat all tracks
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
        self.playback_state().await == PlaybackState::Playing
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

/// Builder for AirPlayPlayer
pub struct PlayerBuilder {
    config: AirPlayConfig,
    auto_reconnect: bool,
}

impl PlayerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: AirPlayConfig::default(),
            auto_reconnect: true,
        }
    }

    /// Set connection timeout
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Set auto-reconnect
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set device name filter
    pub fn device_name(mut self, name: impl Into<String>) -> Self {
        self.config.device_name = Some(name.into());
        self
    }

    /// Build the player
    pub fn build(self) -> AirPlayPlayer {
        let mut player = AirPlayPlayer::with_config(self.config);
        player.auto_reconnect = self.auto_reconnect;
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
/// # Example
///
/// ```rust,no_run
/// use airplay2::quick_play;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// quick_play(vec![
///     ("Never Gonna Give You Up".to_string(), "Rick Astley".to_string()),
/// ]).await?;
/// # Ok(())
/// # }
/// ```
pub async fn quick_play(tracks: Vec<(String, String)>) -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.auto_connect(Duration::from_secs(5)).await?;
    player.play_tracks(tracks).await?;
    Ok(player)
}

/// Quick connect and return player
pub async fn quick_connect() -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.auto_connect(Duration::from_secs(5)).await?;
    Ok(player)
}

/// Quick connect to named device
pub async fn quick_connect_to(name: &str) -> Result<AirPlayPlayer, AirPlayError> {
    let player = AirPlayPlayer::new();
    player.connect_by_name(name, Duration::from_secs(5)).await?;
    Ok(player)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_player_creation() {
        let player = AirPlayPlayer::new();
        assert!(!player.is_connected().await);
    }

    #[tokio::test]
    async fn test_builder() {
        let player = PlayerBuilder::new()
            .connection_timeout(Duration::from_secs(10))
            .auto_reconnect(false)
            .build();

        assert!(!player.auto_reconnect);
    }
}
```

---

### 22.2 Public API (lib.rs)

- [x] **22.2.1** Define public exports

**File:** `src/lib.rs`

```rust
//! AirPlay 2 client library for Rust
//!
//! This library provides a complete implementation for streaming audio to
//! AirPlay 2 compatible devices.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use airplay2::{AirPlayPlayer, quick_connect};
//!
//! # async fn example() -> Result<(), airplay2::AirPlayError> {
//! // Quick connect to first available device
//! let player = quick_connect().await?;
//!
//! // Control playback
//! player.set_volume(0.5).await?;
//! player.play().await?;
//!
//! // Disconnect when done
//! player.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! The library is organized into layers:
//!
//! - **High-level**: `AirPlayPlayer` - Simple, intuitive API
//! - **Mid-level**: `AirPlayClient` - Full control over all features
//! - **Low-level**: Protocol modules - Direct protocol access
//!
//! # Features
//!
//! - Device discovery via mDNS
//! - HomeKit pairing (transient and persistent)
//! - PCM audio streaming
//! - URL-based streaming
//! - Volume control
//! - Multi-room support
//! - Event-driven updates

#![deny(missing_docs)]
#![deny(unsafe_code)]

// Core modules
pub mod types;
pub mod error;
pub mod audio;

// Protocol modules
pub mod protocol;

// Network and connection
pub mod net;
pub mod connection;
pub mod discovery;

// Streaming
pub mod streaming;

// Control
pub mod control;

// State management
pub mod state;

// Multi-room
pub mod multiroom;

// Testing utilities
#[cfg(feature = "testing")]
pub mod testing;

// Main implementations
mod client;
mod player;

// Public exports
pub use client::AirPlayClient;
pub use player::{AirPlayPlayer, PlayerBuilder, quick_play, quick_connect, quick_connect_to};
pub use types::{AirPlayDevice, AirPlayConfig, TrackInfo, PlaybackState, DeviceCapabilities};
pub use error::AirPlayError;
pub use discovery::{scan, discover, DiscoveryEvent};
pub use audio::AudioFormat;
pub use control::volume::Volume;
pub use control::playback::RepeatMode;
pub use state::{ClientState, ClientEvent};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prelude for common imports
pub mod prelude {
    //! Convenient re-exports

    pub use crate::AirPlayPlayer;
    pub use crate::AirPlayClient;
    pub use crate::AirPlayDevice;
    pub use crate::AirPlayConfig;
    pub use crate::AirPlayError;
    pub use crate::TrackInfo;
    pub use crate::PlaybackState;
    pub use crate::Volume;
    pub use crate::AudioFormat;

    pub use crate::quick_connect;
    pub use crate::quick_connect_to;
    pub use crate::quick_play;
    pub use crate::scan;
    pub use crate::discover;
}
```

---

## Acceptance Criteria

- [x] AirPlayPlayer provides simple API
- [x] Quick connect methods work
- [x] Playback controls are intuitive
- [x] Builder pattern is available
- [x] Public API is well-organized
- [x] Prelude exports common types
- [x] All unit tests pass

---

## Notes

- Keep the high-level API stable
- Document all public items
- Consider adding examples crate
- May want async drop support
- Error messages should be user-friendly
