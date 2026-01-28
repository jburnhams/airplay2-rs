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
    /// Audio RTP socket
    pub audio: UdpSocket,
    /// Control RTP socket
    pub control: UdpSocket,
    /// Timing RTP socket
    pub timing: UdpSocket,
}

impl AllocatedSockets {
    /// Get bound ports (audio, control, timing)
    pub fn ports(&self) -> (u16, u16, u16) {
        (
            self.audio.local_addr().map(|a| a.port()).unwrap_or(0),
            self.control.local_addr().map(|a| a.port()).unwrap_or(0),
            self.timing.local_addr().map(|a| a.port()).unwrap_or(0),
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
        // If range is too small, just wrap around or fail?
        // We wrap around.
        if self.next + 3 > self.range {
            self.next = 0;
        }

        let offset = self.next;
        self.next = (self.next + 3) % self.range;

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
    /// Returns error if session cannot be started (e.g. busy)
    pub async fn start_session(&self, client_addr: SocketAddr) -> Result<String, SessionError> {
        let mut active = self.active_session.write().await;

        // Check if session already exists
        if let Some(ref existing) = *active {
            match self.config.preemption_policy {
                PreemptionPolicy::AllowPreempt => {
                    // End existing session
                    let old_id = existing.id().to_string();

                    // Note: We are holding the write lock here, so we can't call end_session
                    // which takes the lock. We must do cleanup manually or release lock.
                    // But we want to replace it atomically.

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
    /// Returns error if sockets cannot be bound
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
    /// Returns error if session not found or transition invalid
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
        let session_to_end = {
            let mut active = self.active_session.write().await;
            active.take()
        };

        if let Some(session) = session_to_end {
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

    /// Check if session should timeout and enforce it
    ///
    /// This method atomically checks timeouts and performs cleanup if needed,
    /// ensuring no locks are held during the async cleanup.
    pub async fn enforce_timeouts(&self) {
        let session_to_end = {
            let mut active = self.active_session.write().await;

            // Determine if we need to kill the session
            let mut reason = None;

            if let Some(ref session) = *active {
                if session.is_timed_out(self.config.idle_timeout) {
                    reason = Some("Idle timeout");
                } else if self.config.max_duration > Duration::ZERO
                    && session.age() > self.config.max_duration
                {
                    reason = Some("Maximum duration exceeded");
                }
            }

            if let Some(r) = reason {
                active.take().map(|s| (s, r))
            } else {
                None
            }
        }; // Write lock dropped here

        if let Some((session, reason)) = session_to_end {
            tracing::info!("Session {} timed out: {}", session.id(), reason);
            self.cleanup_sockets().await;

            let _ = self.event_tx.send(SessionEvent::SessionEnded {
                session_id: session.id().to_string(),
                reason: reason.to_string(),
            });
        }
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
    /// Returns error if no session is active
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

    /// Start background timeout monitor
    ///
    /// Returns a handle that can be used to stop the monitor.
    #[must_use]
    pub fn start_timeout_monitor(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        // Use Weak reference to avoid cycle if the task is not dropped
        let manager_weak = Arc::downgrade(self);
        let check_interval = self.config.idle_timeout / 4;

        tokio::spawn(async move {
            let mut ticker = interval(check_interval);

            loop {
                ticker.tick().await;

                if let Some(manager) = manager_weak.upgrade() {
                    manager.enforce_timeouts().await;
                } else {
                    // Manager dropped, stop monitor
                    break;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
    }

    #[tokio::test]
    async fn test_session_manager_lifecycle() {
        let manager = SessionManager::new(SessionManagerConfig::default());
        let mut events = manager.subscribe();

        // Start
        let id = manager.start_session(test_addr()).await.unwrap();
        assert!(matches!(
            events.recv().await.unwrap(),
            SessionEvent::SessionStarted { .. }
        ));
        assert!(manager.has_active_session().await);
        assert_eq!(manager.current_session_id().await, Some(id));

        // State change
        manager.update_state(SessionState::Announced).await.unwrap();
        assert!(matches!(
            events.recv().await.unwrap(),
            SessionEvent::StateChanged { .. }
        ));

        // End
        manager.end_session("test").await;
        assert!(matches!(
            events.recv().await.unwrap(),
            SessionEvent::SessionEnded { .. }
        ));
        assert!(!manager.has_active_session().await);
    }

    #[tokio::test]
    async fn test_enforce_timeouts() {
        let config = SessionManagerConfig {
            idle_timeout: Duration::from_millis(10), // Very short timeout
            ..Default::default()
        };
        let manager = SessionManager::new(config);
        let mut events = manager.subscribe();

        manager.start_session(test_addr()).await.unwrap();
        let _ = events.recv().await.unwrap(); // Started

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Enforce
        manager.enforce_timeouts().await;

        // Should be ended
        match events.recv().await.unwrap() {
            SessionEvent::SessionEnded { reason, .. } => {
                assert_eq!(reason, "Idle timeout");
            }
            e => panic!("Unexpected event: {:?}", e),
        }
        assert!(!manager.has_active_session().await);
    }
}
