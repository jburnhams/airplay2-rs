//! Main `AirPlay` receiver implementation

use super::config::ReceiverConfig;
use super::events::ReceiverEvent;
use super::session_manager::{SessionManager, SessionManagerConfig};
use super::set_parameter_handler::ParameterUpdate;
use crate::discovery::advertiser::{AdvertiserConfig, AsyncRaopAdvertiser};
use crate::net::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::rtsp::transport::TransportHeader;
use crate::protocol::rtsp::{RtspRequest, RtspServerCodec, encode_response};

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, broadcast, mpsc};

/// `AirPlay` 1 receiver
pub struct AirPlayReceiver {
    config: ReceiverConfig,
    state: Arc<RwLock<ReceiverState>>,
    event_tx: broadcast::Sender<ReceiverEvent>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Receiver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverState {
    /// Receiver is stopped
    Stopped,
    /// Receiver is starting
    Starting,
    /// Receiver is running and accepting connections
    Running,
    /// Receiver is stopping
    Stopping,
}

impl AirPlayReceiver {
    /// Create a new receiver with configuration
    #[must_use]
    pub fn new(config: ReceiverConfig) -> Self {
        let (event_tx, _) = broadcast::channel(64);

        Self {
            config,
            state: Arc::new(RwLock::new(ReceiverState::Stopped)),
            event_tx,
            shutdown_tx: None,
        }
    }

    /// Create with default configuration
    pub fn with_name(name: impl Into<String>) -> Self {
        Self::new(ReceiverConfig::with_name(name))
    }

    /// Subscribe to events
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ReceiverEvent> {
        self.event_tx.subscribe()
    }

    /// Get current state
    pub async fn state(&self) -> ReceiverState {
        *self.state.read().await
    }

    /// Start the receiver
    ///
    /// # Errors
    ///
    /// Returns error if receiver cannot start (e.g. port already in use).
    pub async fn start(&mut self) -> Result<(), ReceiverError> {
        {
            let mut state = self.state.write().await;
            if *state != ReceiverState::Stopped {
                return Err(ReceiverError::AlreadyRunning);
            }
            *state = ReceiverState::Starting;
        }

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Start mDNS advertisement
        let advertiser_config = AdvertiserConfig {
            name: self.config.name.clone(),
            port: self.config.port,
            capabilities: self.config.capabilities.clone(),
            ..Default::default()
        };

        let advertiser = AsyncRaopAdvertiser::start(advertiser_config)
            .await
            .map_err(|e| ReceiverError::Advertisement(e.to_string()))?;

        // Start TCP listener
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.config.port))
            .await
            .map_err(|e| ReceiverError::Network(e.to_string()))?;

        let actual_port = listener.local_addr()?.port();

        // Create session manager
        let session_manager = Arc::new(SessionManager::new(SessionManagerConfig {
            idle_timeout: self.config.session_timeout,
            preemption_policy: if self.config.allow_preemption {
                super::session_manager::PreemptionPolicy::AllowPreempt
            } else {
                super::session_manager::PreemptionPolicy::Reject
            },
            ..Default::default()
        }));

        // Emit started event
        let _ = self.event_tx.send(ReceiverEvent::Started {
            name: self.config.name.clone(),
            port: actual_port,
        });

        *self.state.write().await = ReceiverState::Running;

        // Clone for async task
        let event_tx = self.event_tx.clone();
        let state = self.state.clone();
        let config = self.config.clone();

        // Main server loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let session_manager = session_manager.clone();
                                let event_tx = event_tx.clone();
                                let config = config.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        stream,
                                        addr,
                                        session_manager,
                                        event_tx,
                                        config,
                                    ).await {
                                        tracing::error!("Connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }

            // Cleanup
            advertiser.shutdown().await;
            *state.write().await = ReceiverState::Stopped;
            let _ = event_tx.send(ReceiverEvent::Stopped);
        });

        Ok(())
    }

    /// Stop the receiver
    ///
    /// # Errors
    ///
    /// Returns error if receiver cannot stop (should not happen).
    pub async fn stop(&mut self) -> Result<(), ReceiverError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
            *self.state.write().await = ReceiverState::Stopping;
        }
        Ok(())
    }
}

/// Handle a single client connection
async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    session_manager: Arc<SessionManager>,
    event_tx: broadcast::Sender<ReceiverEvent>,
    config: ReceiverConfig,
) -> Result<(), ReceiverError> {
    let _ = event_tx.send(ReceiverEvent::ClientConnected {
        address: addr,
        user_agent: None,
    });

    // Start session
    let _session_id = session_manager
        .start_session(addr)
        .await
        .map_err(|e| ReceiverError::Session(e.to_string()))?;

    // Use config to setup pipeline later (placeholder to avoid unused warning)
    tracing::debug!("Session started with config: {:?}", config.name);

    let mut codec = RtspServerCodec::new();
    let mut buf = vec![0u8; 4096];

    loop {
        let n = match stream.read(&mut buf).await {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(e) => {
                tracing::error!("Read error: {}", e);
                break;
            }
        };

        codec.feed(&buf[..n]);

        while let Ok(Some(request)) = codec.decode() {
            // Process request
            let mut result = session_manager
                .with_session(|session| {
                    crate::receiver::rtsp_handler::handle_request(
                        &request, session, None, // rsa_private_key, pass if needed (TODO)
                    )
                })
                .await
                .map_err(|e| ReceiverError::Session(e.to_string()))?;

            // Handle parameter updates
            process_parameter_updates(&result.parameter_updates, &session_manager, &event_tx).await;

            // Handle port allocation for SETUP
            if let Some(ref ports_req) = result.allocated_ports {
                handle_setup_ports(
                    ports_req,
                    &request,
                    &mut result.response,
                    &session_manager,
                    addr,
                )
                .await?;
            }

            // Send response
            let response_bytes = encode_response(&result.response);
            if stream.write_all(&response_bytes).await.is_err() {
                break;
            }

            // Handle state changes
            if let Some(new_state) = result.new_state {
                let _ = session_manager.update_state(new_state).await;

                match new_state {
                    super::session::SessionState::Streaming => {
                        let _ = event_tx.send(ReceiverEvent::PlaybackStarted);
                    }
                    super::session::SessionState::Paused => {
                        let _ = event_tx.send(ReceiverEvent::PlaybackPaused);
                    }
                    super::session::SessionState::Teardown => {
                        let _ = event_tx.send(ReceiverEvent::PlaybackStopped);
                    }
                    _ => {}
                }
            }

            if result.stop_streaming {
                break;
            }
        }
    }

    // Cleanup
    session_manager.end_session("Connection closed").await;
    let _ = event_tx.send(ReceiverEvent::ClientDisconnected {
        address: addr,
        reason: "Connection closed".to_string(),
    });

    Ok(())
}

async fn process_parameter_updates(
    updates: &[ParameterUpdate],
    session_manager: &SessionManager,
    event_tx: &broadcast::Sender<ReceiverEvent>,
) {
    for update in updates {
        match update {
            ParameterUpdate::Volume(vol_update) => {
                // Update session volume
                let vol_db = vol_update.db;
                session_manager.set_volume(vol_db).await;

                let _ = event_tx.send(ReceiverEvent::VolumeChanged {
                    db: vol_db,
                    linear: vol_update.linear,
                    muted: vol_update.muted,
                });
            }
            ParameterUpdate::Metadata(metadata) => {
                let _ = event_tx.send(ReceiverEvent::MetadataUpdated(metadata.clone()));
            }
            ParameterUpdate::Progress(progress) => {
                let _ = event_tx.send(ReceiverEvent::ProgressUpdated(*progress));
            }
            ParameterUpdate::Artwork(artwork) => {
                let _ = event_tx.send(ReceiverEvent::ArtworkUpdated(artwork.clone()));
            }
            ParameterUpdate::Unknown(_) => {}
        }
    }
}

async fn handle_setup_ports(
    ports_req: &crate::receiver::rtsp_handler::AllocatedPorts,
    request: &RtspRequest,
    response: &mut crate::protocol::rtsp::RtspResponse,
    session_manager: &SessionManager,
    addr: SocketAddr,
) -> Result<(), ReceiverError> {
    let (audio_port, control_port, timing_port) = session_manager
        .allocate_sockets()
        .await
        .map_err(|e| ReceiverError::Network(e.to_string()))?;

    // Store sockets and client info in session
    let _ = session_manager
        .with_session(|session| {
            session.set_sockets(crate::receiver::session::SessionSockets {
                audio_port,
                control_port,
                timing_port,
                client_control_port: ports_req.client_control_port,
                client_timing_port: ports_req.client_timing_port,
                client_addr: Some(addr),
            });
        })
        .await;

    // Update Transport header in response
    if let Some(transport_str) = request.headers.get("Transport") {
        if let Ok(transport) = TransportHeader::parse(transport_str) {
            let new_header = transport.to_response_header(audio_port, control_port, timing_port);
            response.headers.insert("Transport".to_string(), new_header);
        }
    }
    Ok(())
}

/// Receiver errors
#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    /// Receiver already running
    #[error("Receiver already running")]
    AlreadyRunning,

    /// Advertisement error
    #[error("Advertisement error: {0}")]
    Advertisement(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Session error
    #[error("Session error: {0}")]
    Session(String),

    /// Audio error
    #[error("Audio error: {0}")]
    Audio(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
