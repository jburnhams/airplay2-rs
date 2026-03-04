use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};

use super::advertisement::Ap2ServiceAdvertiser;
use super::config::Ap2Config;
use crate::protocol::crypto::Ed25519KeyPair;

/// `AirPlay` 2 Receiver
///
/// High-level API for receiving `AirPlay` 2 audio streams.
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::receiver::ap2::{AirPlay2Receiver, Ap2Config, ReceiverEvent};
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
    identity: Ed25519KeyPair,
    state: Arc<RwLock<ReceiverState>>,
    event_tx: broadcast::Sender<ReceiverEvent>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    advertiser: Option<Ap2ServiceAdvertiser>,
}

/// Receiver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverState {
    /// Receiver is stopped and not accepting connections
    Stopped,
    /// Receiver is currently starting up
    Starting,
    /// Receiver is running and accepting connections
    Running,
    /// Receiver is in the process of stopping
    Stopping,
}

/// Events emitted by the receiver
#[derive(Debug, Clone)]
pub enum ReceiverEvent {
    /// Receiver started
    Started,
    /// Client connected
    Connected {
        /// The peer address
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
        /// The audio samples
        samples: Vec<i16>,
        /// The sample rate
        sample_rate: u32,
    },
    /// Volume changed
    VolumeChanged {
        /// The new volume in decibels
        volume_db: f32,
    },
    /// Metadata updated
    MetadataUpdated {
        /// The updated title
        title: Option<String>,
        /// The updated artist
        artist: Option<String>,
    },
    /// Artwork available
    ArtworkUpdated {
        /// The artwork image data
        data: Vec<u8>,
        /// The MIME type of the artwork
        mime_type: String,
    },
    /// Client disconnected
    Disconnected,
    /// Receiver stopped
    Stopped,
    /// Error occurred
    Error {
        /// The error message
        message: String,
    },
}

impl AirPlay2Receiver {
    /// Create a new receiver with the given configuration
    ///
    /// # Errors
    /// Returns an error if the receiver identity cannot be generated.
    pub fn new(config: Ap2Config) -> Result<Self, ReceiverError> {
        let identity = Ed25519KeyPair::generate();
        let (event_tx, _) = broadcast::channel(100);

        Ok(Self {
            config,
            identity,
            state: Arc::new(RwLock::new(ReceiverState::Stopped)),
            event_tx,
            shutdown_tx: None,
            advertiser: None,
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
    /// Returns an error if the receiver is already running, or if advertisement or TCP listener
    /// creation fails.
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
        let public_key = *self.identity.public_key().as_bytes();
        let advertiser = Ap2ServiceAdvertiser::new(self.config.clone(), public_key)
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;
        advertiser
            .start()
            .await
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;
        self.advertiser = Some(advertiser);

        // Start TCP listener
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.config.server_port))
            .await
            .map_err(ReceiverError::Io)?;

        self.config.server_port = listener.local_addr().map_err(ReceiverError::Io)?.port();

        tracing::info!(
            "AirPlay 2 receiver listening on port {}",
            self.config.server_port
        );

        // Update state
        *self.state.write().await = ReceiverState::Running;
        let _ = self.event_tx.send(ReceiverEvent::Started);

        // Start accept loop
        let event_tx_clone = self.event_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((_stream, peer_addr)) => {
                                tracing::debug!("Accepted connection from {}", peer_addr);
                                let _ = event_tx_clone.send(ReceiverEvent::Connected {
                                    peer: peer_addr.to_string(),
                                });
                                // Further handling of `_stream` would be implemented here
                                // such as wrapping in an HTTP/RTSP server session
                            }
                            Err(e) => {
                                tracing::error!("Failed to accept connection: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Shutting down accept loop");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the receiver
    ///
    /// # Errors
    /// Returns an error if stopping fails (currently this always succeeds if it isn't already
    /// stopped).
    pub async fn stop(&mut self) -> Result<(), ReceiverError> {
        let mut state = self.state.write().await;
        if *state == ReceiverState::Stopped {
            return Ok(());
        }
        *state = ReceiverState::Stopping;
        drop(state);

        // Stop advertisement
        if let Some(advertiser) = self.advertiser.take() {
            let _ = advertiser.stop().await;
        }

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

/// Receiver error types
#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    /// Attempted to start an already running receiver
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

/// Builder for `AirPlay2Receiver`
pub struct ReceiverBuilder {
    config: Ap2Config,
}

impl ReceiverBuilder {
    /// Create a new builder with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: Ap2Config::new(name),
        }
    }

    /// Set the optional password for the receiver
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.config.password = Some(password.into());
        self
    }

    /// Set the TCP port for the receiver
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    /// Enable or disable multi-room support
    #[must_use]
    pub fn multi_room(mut self, enabled: bool) -> Self {
        self.config.multi_room_enabled = enabled;
        self
    }

    /// Build the receiver
    ///
    /// # Errors
    /// Returns an error if the receiver identity cannot be generated.
    pub fn build(self) -> Result<AirPlay2Receiver, ReceiverError> {
        AirPlay2Receiver::new(self.config)
    }
}
