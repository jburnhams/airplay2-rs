//! High-Level `AirPlay` 2 Receiver API

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};

use super::advertisement::Ap2ServiceAdvertiser;
use super::config::Ap2Config;
use crate::protocol::crypto::Ed25519KeyPair;

/// Receiver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverState {
    /// Receiver is stopped
    Stopped,
    /// Receiver is starting
    Starting,
    /// Receiver is running
    Running,
    /// Receiver is stopping
    Stopping,
}

/// Events emitted by the receiver
#[derive(Debug, Clone)]
pub enum ReceiverEvent {
    /// Receiver started
    Started,
    /// Client connected
    Connected {
        /// Peer address
        peer: String,
    },
    /// Pairing in progress
    PairingStarted,
    /// Pairing completed
    PairingComplete,
    /// Streaming started
    StreamingStarted,
    /// Audio data available
    AudioData {
        /// PCM samples
        samples: Vec<i16>,
        /// Sample rate
        sample_rate: u32,
    },
    /// Volume changed
    VolumeChanged {
        /// Volume in dB
        volume_db: f32,
    },
    /// Metadata updated
    MetadataUpdated {
        /// Track title
        title: Option<String>,
        /// Track artist
        artist: Option<String>,
    },
    /// Artwork available
    ArtworkUpdated {
        /// Artwork image data
        data: Vec<u8>,
        /// MIME type
        mime_type: String,
    },
    /// Client disconnected
    Disconnected,
    /// Receiver stopped
    Stopped,
    /// Error occurred
    Error {
        /// Error message
        message: String,
    },
}

/// Errors from the `AirPlay2Receiver`
#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    /// Receiver is already running
    #[error("Receiver already running")]
    AlreadyRunning,

    /// Error during mDNS advertisement
    #[error("Advertisement error: {0}")]
    Advertisement(String),

    /// I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Session error
    #[error("Session error: {0}")]
    Session(String),
}

/// `AirPlay` 2 Receiver
///
/// High-level API for receiving `AirPlay` 2 audio streams.
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::receiver::ap2::config::Ap2Config;
/// use airplay2::receiver::ap2::receiver::{AirPlay2Receiver, ReceiverEvent};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = Ap2Config::new("My Speaker").with_password("secret123");
///
///     let mut receiver = AirPlay2Receiver::new(config)?;
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
///             ReceiverEvent::AudioData {
///                 samples,
///                 sample_rate,
///             } => { /* play audio */ }
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
    #[allow(dead_code)]
    identity: Ed25519KeyPair,
    state: Arc<RwLock<ReceiverState>>,
    event_tx: broadcast::Sender<ReceiverEvent>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl AirPlay2Receiver {
    /// Create a new receiver with the given configuration
    ///
    /// # Errors
    /// Returns a `ReceiverError` if the initialization fails.
    pub fn new(config: Ap2Config) -> Result<Self, ReceiverError> {
        let identity = Ed25519KeyPair::generate();
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
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ReceiverEvent> {
        self.event_tx.subscribe()
    }

    /// Start the receiver
    ///
    /// # Errors
    /// Returns a `ReceiverError` if the receiver is already running or
    /// if there is an error during starting components.
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
        advertiser
            .start()
            .await
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;

        // Start TCP listener
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.config.server_port))
            .await
            .map_err(ReceiverError::Io)?;

        tracing::info!(
            "AirPlay 2 receiver listening on port {}",
            self.config.server_port
        );

        // Update state
        *self.state.write().await = ReceiverState::Running;
        let _ = self.event_tx.send(ReceiverEvent::Started);

        // Start accept loop
        let mut shutdown_rx = shutdown_tx.subscribe();
        let event_tx_clone = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Ok((mut _stream, peer_addr)) = listener.accept() => {
                        tracing::debug!("Accepted connection from {}", peer_addr);
                        let _ = event_tx_clone.send(ReceiverEvent::Connected {
                            peer: peer_addr.to_string(),
                        });

                        // We would normally spawn a connection handler here
                        // For this high-level API implementation, this indicates the framework
                        // is ready to be expanded in the future.
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::debug!("Accept loop shutting down");
                        break;
                    }
                    else => break,
                }
            }
        });

        Ok(())
    }

    /// Stop the receiver
    ///
    /// # Errors
    /// Returns a `ReceiverError` if an error occurs while stopping.
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

        tracing::info!("AirPlay 2 receiver stopped");
        Ok(())
    }

    /// Get current state
    pub async fn state(&self) -> ReceiverState {
        *self.state.read().await
    }

    /// Get the configuration
    #[must_use]
    pub fn config(&self) -> &Ap2Config {
        &self.config
    }
}

/// Builder for `AirPlay2Receiver`
pub struct ReceiverBuilder {
    config: Ap2Config,
}

impl ReceiverBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: Ap2Config::new(name),
        }
    }

    /// Set a password for the receiver
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.config.password = Some(password.into());
        self
    }

    /// Set the port to listen on
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    /// Set multi-room support
    #[must_use]
    pub fn multi_room(mut self, enabled: bool) -> Self {
        self.config.multi_room_enabled = enabled;
        self
    }

    /// Build the receiver
    ///
    /// # Errors
    /// Returns a `ReceiverError` if the receiver cannot be built.
    pub fn build(self) -> Result<AirPlay2Receiver, ReceiverError> {
        AirPlay2Receiver::new(self.config)
    }
}
