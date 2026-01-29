# Section 60: Receiver Integration

## Dependencies
- All previous AirPlay 2 receiver sections (46-59)

## Overview

This section integrates all receiver components into a unified `AirPlay2Receiver` API that applications can use to accept AirPlay connections and receive audio.

## Objectives

- Wire all components together
- Provide high-level API similar to `AirPlayPlayer`
- Handle session lifecycle
- Emit events for application integration
- Support graceful shutdown

---

## Tasks

### 60.1 High-Level Receiver API

**File:** `src/receiver/ap2/receiver.rs`

```rust
//! High-Level AirPlay 2 Receiver API

use super::config::Ap2Config;
use super::advertisement::Ap2ServiceAdvertiser;
use super::pairing_server::PairingServer;
use super::encrypted_rtsp::EncryptedRtspCodec;
use super::setup_handler::SetupHandler;
use super::rtp_receiver::{RtpReceiver, RtpReceiverConfig};
use super::jitter_buffer::JitterBuffer;
use super::volume_handler::VolumeController;
use super::metadata_handler::MetadataController;
use super::ptp_clock::PtpClock;
use super::capabilities::DeviceCapabilities;
use super::info_endpoint::InfoEndpoint;

use crate::protocol::crypto::ed25519::Ed25519Keypair;
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast, RwLock};
use tokio::net::TcpListener;

/// AirPlay 2 Receiver
///
/// High-level API for receiving AirPlay 2 audio streams.
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::receiver::ap2::{AirPlay2Receiver, Ap2Config, ReceiverEvent};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = Ap2Config::new("My Speaker")
///         .with_password("secret123");
///
///     let receiver = AirPlay2Receiver::new(config)?;
///
///     // Subscribe to events
///     let mut events = receiver.subscribe();
///
///     // Start receiver
///     receiver.start().await?;
///
///     // Handle events
///     while let Ok(event) = events.recv().await {
///         match event {
///             ReceiverEvent::Connected { peer } => println!("Connected: {}", peer),
///             ReceiverEvent::AudioData { samples } => { /* play audio */ }
///             ReceiverEvent::Disconnected => break,
///             _ => {}
///         }
///     }
///
///     receiver.stop().await?;
///     Ok(())
/// }
/// ```
pub struct AirPlay2Receiver {
    config: Ap2Config,
    identity: Ed25519Keypair,
    state: Arc<RwLock<ReceiverState>>,
    event_tx: broadcast::Sender<ReceiverEvent>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

/// Receiver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

/// Events emitted by the receiver
#[derive(Debug, Clone)]
pub enum ReceiverEvent {
    /// Receiver started
    Started,
    /// Client connected
    Connected { peer: String },
    /// Pairing in progress
    PairingStarted,
    /// Pairing completed
    PairingComplete,
    /// Streaming started
    StreamingStarted,
    /// Audio data available
    AudioData { samples: Vec<i16>, sample_rate: u32 },
    /// Volume changed
    VolumeChanged { volume_db: f32 },
    /// Metadata updated
    MetadataUpdated { title: Option<String>, artist: Option<String> },
    /// Artwork available
    ArtworkUpdated { data: Vec<u8>, mime_type: String },
    /// Client disconnected
    Disconnected,
    /// Receiver stopped
    Stopped,
    /// Error occurred
    Error { message: String },
}

impl AirPlay2Receiver {
    /// Create a new receiver with the given configuration
    pub fn new(config: Ap2Config) -> Result<Self, ReceiverError> {
        let identity = Ed25519Keypair::generate();
        let (event_tx, _) = broadcast::channel(100);

        Ok(Self {
            config,
            identity,
            state: Arc::new(RwLock::new(ReceiverState::Stopped)),
            event_tx,
            shutdown_tx: None,
        })
    }

    /// Subscribe to receiver events
    pub fn subscribe(&self) -> broadcast::Receiver<ReceiverEvent> {
        self.event_tx.subscribe()
    }

    /// Start the receiver
    pub async fn start(&mut self) -> Result<(), ReceiverError> {
        let mut state = self.state.write().await;
        if *state != ReceiverState::Stopped {
            return Err(ReceiverError::AlreadyRunning);
        }
        *state = ReceiverState::Starting;
        drop(state);

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Start mDNS advertisement
        let advertiser = Ap2ServiceAdvertiser::new(self.config.clone())
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;
        advertiser.start().await
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;

        // Start TCP listener
        let listener = TcpListener::bind(
            format!("0.0.0.0:{}", self.config.server_port)
        ).await.map_err(ReceiverError::Io)?;

        log::info!("AirPlay 2 receiver listening on port {}", self.config.server_port);

        // Update state
        *self.state.write().await = ReceiverState::Running;
        let _ = self.event_tx.send(ReceiverEvent::Started);

        // Start accept loop (would be spawned in real implementation)
        // For brevity, this is simplified

        Ok(())
    }

    /// Stop the receiver
    pub async fn stop(&mut self) -> Result<(), ReceiverError> {
        let mut state = self.state.write().await;
        if *state == ReceiverState::Stopped {
            return Ok(());
        }
        *state = ReceiverState::Stopping;
        drop(state);

        // Signal shutdown
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        *self.state.write().await = ReceiverState::Stopped;
        let _ = self.event_tx.send(ReceiverEvent::Stopped);

        log::info!("AirPlay 2 receiver stopped");
        Ok(())
    }

    /// Get current state
    pub async fn state(&self) -> ReceiverState {
        *self.state.read().await
    }

    /// Get the configuration
    pub fn config(&self) -> &Ap2Config {
        &self.config
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("Receiver already running")]
    AlreadyRunning,

    #[error("Advertisement error: {0}")]
    Advertisement(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Session error: {0}")]
    Session(String),
}

/// Builder for AirPlay2Receiver
pub struct ReceiverBuilder {
    config: Ap2Config,
}

impl ReceiverBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: Ap2Config::new(name),
        }
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.config.password = Some(password.into());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    pub fn multi_room(mut self, enabled: bool) -> Self {
        self.config.multi_room_enabled = enabled;
        self
    }

    pub fn build(self) -> Result<AirPlay2Receiver, ReceiverError> {
        AirPlay2Receiver::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_receiver_creation() {
        let config = Ap2Config::new("Test Speaker");
        let receiver = AirPlay2Receiver::new(config).unwrap();

        assert_eq!(receiver.state().await, ReceiverState::Stopped);
    }

    #[tokio::test]
    async fn test_builder() {
        let receiver = ReceiverBuilder::new("Test Speaker")
            .password("secret")
            .port(7001)
            .build()
            .unwrap();

        assert_eq!(receiver.config().server_port, 7001);
        assert!(receiver.config().password.is_some());
    }
}
```

---

## Acceptance Criteria

- [ ] Unified high-level API
- [ ] Event subscription mechanism
- [ ] Start/stop lifecycle
- [ ] Builder pattern for configuration
- [ ] Graceful shutdown
- [ ] All unit tests pass

---

## References

- [Section 23: High-Level Player API](./complete/23-high-level-player-api.md)
