//! Session manager for the receiver
//!
//! Manages session lifecycle, enforces single-session policy,
//! and handles session preemption.

use super::session::{ReceiverSession, SessionError, SessionState};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::time::interval;

/// Session preemption policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreemptionPolicy {
    /// Reject new connections while session is active
    Reject,
    /// Allow new connection to preempt existing session
    AllowPreempt,
    /// Queue new connection until current session ends
    Queue,
}

/// Session manager configuration
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    /// Session idle timeout
    pub idle_timeout: Duration,
    /// Maximum session duration (0 = unlimited)
    pub max_duration: Duration,
    /// Preemption policy
    pub preemption_policy: PreemptionPolicy,
    /// Base port for UDP sockets
    pub udp_base_port: u16,
    /// Port range size
    pub udp_port_range: u16,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(60),
            max_duration: Duration::ZERO, // Unlimited
            preemption_policy: PreemptionPolicy::AllowPreempt,
            udp_base_port: 6000,
            udp_port_range: 100,
        }
    }
}

/// Events from session manager
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// New session started
    SessionStarted {
        /// Session ID
        session_id: String,
        /// Client address
        client: SocketAddr,
    },
    /// Session state changed
    StateChanged {
        /// Session ID
        session_id: String,
        /// New state
        new_state: SessionState,
    },
    /// Session ended
    SessionEnded {
        /// Session ID
        session_id: String,
        /// Reason for ending
        reason: String,
    },
    /// Volume changed
    VolumeChanged {
        /// Session ID
        session_id: String,
        /// New volume in dB
        volume: f32,
    },
}

/// Manages receiver sessions
pub struct SessionManager {
    config: SessionManagerConfig,
    /// Current active session (only one allowed)
    active_session: Arc<RwLock<Option<ReceiverSession>>>,
    /// Allocated UDP sockets for current session
    sockets: Arc<Mutex<Option<AllocatedSockets>>>,
    /// Port allocator
    port_allocator: Arc<Mutex<PortAllocator>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<SessionEvent>,
}

/// Allocated UDP sockets for a session
pub struct AllocatedSockets {
    /// Audio data socket
    pub audio: UdpSocket,
    /// Control socket
    pub control: UdpSocket,
    /// Timing socket
    pub timing: UdpSocket,
}

impl AllocatedSockets {
    /// Get the ports for the allocated sockets
    ///
    /// Returns (`audio_port`, `control_port`, `timing_port`)
    #[must_use]
    pub fn ports(&self) -> (u16, u16, u16) {
        (
            self.audio.local_addr().map_or(0, |a| a.port()),
            self.control.local_addr().map_or(0, |a| a.port()),
            self.timing.local_addr().map_or(0, |a| a.port()),
        )
    }
}

/// Simple port allocator
struct PortAllocator {
    base: u16,
    range: u16,
    next: u16,
}

impl PortAllocator {
    fn new(base: u16, range: u16) -> Self {
        Self {
            base,
            range,
            next: 0,
        }
    }

    /// Allocate next available port trio
    fn allocate_trio(&mut self) -> (u16, u16, u16) {
        // Ensure we don't go out of bounds
        if self.next + 3 > self.range {
            self.next = 0;
        }

        let offset = self.next;
        self.next += 3;

        (
            self.base + offset,
            self.base + offset + 1,
            self.base + offset + 2,
        )
    }
}

impl SessionManager {
    /// Create a new session manager
    #[must_use]
    pub fn new(config: SessionManagerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(64);

        Self {
            port_allocator: Arc::new(Mutex::new(PortAllocator::new(
                config.udp_base_port,
                config.udp_port_range,
            ))),
            config,
            active_session: Arc::new(RwLock::new(None)),
            sockets: Arc::new(Mutex::new(None)),
            event_tx,
        }
    }

    /// Subscribe to session events
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    /// Check if a session is currently active
    pub async fn has_active_session(&self) -> bool {
        self.active_session.read().await.is_some()
    }

    /// Get current session info (if any)
    pub async fn current_session_id(&self) -> Option<String> {
        self.active_session
            .read()
            .await
            .as_ref()
            .map(|s| s.id().to_string())
    }

    /// Start a new session
    ///
    /// # Errors
    /// Returns `SessionError::Busy` if another session is active and cannot be preempted.
    pub async fn start_session(&self, client_addr: SocketAddr) -> Result<String, SessionError> {
        let mut active = self.active_session.write().await;

        // Check if session already exists
        if let Some(ref existing) = *active {
            match self.config.preemption_policy {
                PreemptionPolicy::AllowPreempt => {
                    // End existing session
                    let old_id = existing.id().to_string();
                    self.cleanup_sockets().await;

                    let _ = self.event_tx.send(SessionEvent::SessionEnded {
                        session_id: old_id,
                        reason: "Preempted by new connection".to_string(),
                    });
                }
                PreemptionPolicy::Reject | PreemptionPolicy::Queue => {
                    // For now, treat as reject (queue not implemented)
                    return Err(SessionError::Busy);
                }
            }
        }

        // Create new session
        let session = ReceiverSession::new(client_addr);
        let session_id = session.id().to_string();

        let _ = self.event_tx.send(SessionEvent::SessionStarted {
            session_id: session_id.clone(),
            client: client_addr,
        });

        *active = Some(session);
        Ok(session_id)
    }

    /// Allocate UDP sockets for the session
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket binding fails.
    pub async fn allocate_sockets(&self) -> Result<(u16, u16, u16), std::io::Error> {
        let (audio_port, control_port, timing_port) = {
            let mut allocator = self.port_allocator.lock().await;
            allocator.allocate_trio()
        };

        // Bind sockets
        let audio = UdpSocket::bind(format!("0.0.0.0:{audio_port}")).await?;
        let control = UdpSocket::bind(format!("0.0.0.0:{control_port}")).await?;
        let timing = UdpSocket::bind(format!("0.0.0.0:{timing_port}")).await?;

        let ports = (
            audio.local_addr()?.port(),
            control.local_addr()?.port(),
            timing.local_addr()?.port(),
        );

        let mut sockets = self.sockets.lock().await;
        *sockets = Some(AllocatedSockets {
            audio,
            control,
            timing,
        });

        Ok(ports)
    }

    /// Get reference to allocated sockets
    #[must_use]
    pub fn get_sockets(&self) -> Option<Arc<Mutex<Option<AllocatedSockets>>>> {
        // Return clone of Arc for shared access
        Some(self.sockets.clone())
    }

    /// Update session state
    ///
    /// # Errors
    /// Returns `SessionError::NotFound` if no active session exists.
    /// Returns `SessionError::InvalidTransition` if the state transition is not allowed.
    pub async fn update_state(&self, new_state: SessionState) -> Result<(), SessionError> {
        let mut active = self.active_session.write().await;

        let session = active
            .as_mut()
            .ok_or_else(|| SessionError::NotFound("No active session".into()))?;

        session.set_state(new_state)?;

        let session_id = session.id().to_string();

        let _ = self.event_tx.send(SessionEvent::StateChanged {
            session_id,
            new_state,
        });

        Ok(())
    }

    /// Update session volume
    pub async fn set_volume(&self, volume: f32) {
        let mut active = self.active_session.write().await;

        if let Some(ref mut session) = *active {
            session.set_volume(volume);

            let _ = self.event_tx.send(SessionEvent::VolumeChanged {
                session_id: session.id().to_string(),
                volume,
            });
        }
    }

    /// End the current session
    pub async fn end_session(&self, reason: &str) {
        let mut active = self.active_session.write().await;

        if let Some(session) = active.take() {
            self.cleanup_sockets().await;

            let _ = self.event_tx.send(SessionEvent::SessionEnded {
                session_id: session.id().to_string(),
                reason: reason.to_string(),
            });
        }
    }

    /// Cleanup UDP sockets
    async fn cleanup_sockets(&self) {
        let mut sockets = self.sockets.lock().await;
        *sockets = None;
        // Sockets are dropped, ports released
    }

    /// Check for session timeout
    pub async fn check_timeout(&self) -> bool {
        let active = self.active_session.read().await;

        if let Some(ref session) = *active {
            if session.is_timed_out(self.config.idle_timeout) {
                return true;
            }
        }

        false
    }

    /// Touch session to reset idle timeout
    pub async fn touch_session(&self) {
        let mut active = self.active_session.write().await;

        if let Some(ref mut session) = *active {
            session.touch();
        }
    }

    /// Run with mutable access to session
    ///
    /// # Errors
    /// Returns `SessionError::NotFound` if no active session exists.
    pub async fn with_session<F, R>(&self, f: F) -> Result<R, SessionError>
    where
        F: FnOnce(&mut ReceiverSession) -> R,
    {
        let mut active = self.active_session.write().await;
        let session = active
            .as_mut()
            .ok_or_else(|| SessionError::NotFound("No active session".into()))?;
        Ok(f(session))
    }

    /// Enforce timeouts atomically
    pub async fn enforce_timeouts(&self) {
        let mut active = self.active_session.write().await;
        let mut should_end = false;
        let mut reason = String::new();

        if let Some(ref session) = *active {
            if session.is_timed_out(self.config.idle_timeout) {
                should_end = true;
                reason = "Idle timeout".to_string();
            } else if self.config.max_duration > Duration::ZERO
                && session.age() > self.config.max_duration
            {
                should_end = true;
                reason = "Maximum duration exceeded".to_string();
            }
        }

        if should_end {
            if let Some(session) = active.take() {
                self.cleanup_sockets().await;

                let _ = self.event_tx.send(SessionEvent::SessionEnded {
                    session_id: session.id().to_string(),
                    reason,
                });
            }
        }
    }

    /// Start background timeout monitor
    ///
    /// Returns a handle that can be used to stop the monitor.
    #[must_use]
    pub fn start_timeout_monitor(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager_weak = Arc::downgrade(self);
        let check_interval = self.config.idle_timeout / 4;

        tokio::spawn(async move {
            let mut ticker = interval(check_interval);

            loop {
                ticker.tick().await;

                if let Some(manager) = manager_weak.upgrade() {
                    manager.enforce_timeouts().await;
                } else {
                    break;
                }
            }
        })
    }
}
