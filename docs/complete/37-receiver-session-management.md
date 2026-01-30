# Section 37: Receiver Session Management

**VERIFIED**: ReceiverSession, SessionState, SessionManager, PreemptionPolicy, AllocatedSockets checked against source.

## Dependencies
- **Section 36**: RTSP Server (Sans-IO) (RTSP handling)
- **Section 34**: Receiver Overview (architecture)
- **Section 02**: Core Types, Errors & Config

## Overview

This section implements session management for the AirPlay 1 receiver. A "session" represents an active connection from an AirPlay sender, encompassing:

- TCP connection for RTSP control
- UDP sockets for audio, control, and timing
- Session state machine
- Stream parameters (codec, encryption keys)
- Playback state (volume, metadata)

The receiver enforces **single-session semantics**: only one sender can stream at a time. New connections either queue, reject, or preempt the existing session based on configuration.

## Objectives

- Implement session state machine with all RAOP states
- Manage UDP socket lifecycle (allocation, binding, cleanup)
- Store stream parameters from ANNOUNCE/SETUP
- Track session timeout and keep-alive
- Support session preemption policies
- Provide async session manager for connection handling

---

## Tasks

### 37.1 Session State Machine

- [x] **37.1.1** Define session states and transitions

**File:** `src/receiver/session.rs`

```rust
//! Receiver session management
//!
//! Manages the lifecycle of an AirPlay streaming session from
//! connection through teardown.

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// Session states following RAOP protocol flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state after TCP connection
    Connected,
    /// ANNOUNCE received, stream parameters known
    Announced,
    /// SETUP complete, UDP ports allocated
    Setup,
    /// RECORD received, actively streaming
    Streaming,
    /// PAUSE received, stream paused but session alive
    Paused,
    /// TEARDOWN received or connection lost
    Teardown,
    /// Session ended, ready for cleanup
    Closed,
}

impl SessionState {
    /// Check if transition to new state is valid
    pub fn can_transition_to(&self, new_state: SessionState) -> bool {
        use SessionState::*;

        match (self, new_state) {
            // Normal forward flow
            (Connected, Announced) => true,
            (Announced, Setup) => true,
            (Setup, Streaming) => true,
            (Streaming, Paused) => true,
            (Paused, Streaming) => true,

            // Teardown from any active state
            (Connected | Announced | Setup | Streaming | Paused, Teardown) => true,
            (Teardown, Closed) => true,

            // Allow re-announce (some senders do this)
            (Announced, Announced) => true,
            (Setup, Announced) => true,

            // Allow re-setup
            (Setup, Setup) => true,

            _ => false,
        }
    }

    /// Is this an active streaming state?
    pub fn is_active(&self) -> bool {
        matches!(self, SessionState::Streaming | SessionState::Paused)
    }

    /// Is the session still valid (not closed)?
    pub fn is_valid(&self) -> bool {
        !matches!(self, SessionState::Teardown | SessionState::Closed)
    }
}

/// Stream parameters parsed from ANNOUNCE SDP
#[derive(Debug, Clone)]
pub struct StreamParameters {
    /// Audio codec
    pub codec: AudioCodec,
    /// Sample rate (typically 44100)
    pub sample_rate: u32,
    /// Bits per sample (typically 16)
    pub bits_per_sample: u8,
    /// Number of channels (typically 2)
    pub channels: u8,
    /// Samples per RTP packet (typically 352)
    pub frames_per_packet: u32,
    /// AES key (decrypted from RSA, if encryption used)
    pub aes_key: Option<[u8; 16]>,
    /// AES IV (if encryption used)
    pub aes_iv: Option<[u8; 16]>,
    /// Minimum latency requested by sender (in samples)
    pub min_latency: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Pcm,      // L16
    Alac,     // Apple Lossless
    AacLc,    // AAC Low Complexity
    AacEld,   // AAC Enhanced Low Delay
}

impl Default for StreamParameters {
    fn default() -> Self {
        Self {
            codec: AudioCodec::Alac,
            sample_rate: 44100,
            bits_per_sample: 16,
            channels: 2,
            frames_per_packet: 352,
            aes_key: None,
            aes_iv: None,
            min_latency: None,
        }
    }
}

/// UDP socket addresses for a session
#[derive(Debug, Clone)]
pub struct SessionSockets {
    /// Our audio receive port
    pub audio_port: u16,
    /// Our control port (sync packets)
    pub control_port: u16,
    /// Our timing port (NTP-like)
    pub timing_port: u16,
    /// Client's control port (for sending retransmit requests)
    pub client_control_port: Option<u16>,
    /// Client's timing port
    pub client_timing_port: Option<u16>,
    /// Client's address
    pub client_addr: Option<SocketAddr>,
}

/// A receiver session
#[derive(Debug)]
pub struct ReceiverSession {
    /// Unique session identifier
    id: String,
    /// Current state
    state: SessionState,
    /// Client address
    client_addr: SocketAddr,
    /// Stream parameters (set after ANNOUNCE)
    stream_params: Option<StreamParameters>,
    /// Socket configuration (set after SETUP)
    sockets: Option<SessionSockets>,
    /// Current volume (-144.0 to 0.0 dB)
    volume: f32,
    /// Last activity timestamp
    last_activity: Instant,
    /// Session creation time
    created_at: Instant,
    /// RTSP session ID sent to client
    rtsp_session_id: Option<String>,
    /// Initial RTP sequence number
    initial_seq: Option<u16>,
    /// Initial RTP timestamp
    initial_rtptime: Option<u32>,
}

impl ReceiverSession {
    /// Create a new session
    pub fn new(client_addr: SocketAddr) -> Self {
        Self {
            id: generate_session_id(),
            state: SessionState::Connected,
            client_addr,
            stream_params: None,
            sockets: None,
            volume: 0.0,  // Full volume
            last_activity: Instant::now(),
            created_at: Instant::now(),
            rtsp_session_id: None,
            initial_seq: None,
            initial_rtptime: None,
        }
    }

    /// Get session ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Set state (validates transition)
    pub fn set_state(&mut self, new_state: SessionState) -> Result<(), SessionError> {
        if !self.state.can_transition_to(new_state) {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: new_state,
            });
        }
        self.state = new_state;
        self.touch();
        Ok(())
    }

    /// Get client address
    pub fn client_addr(&self) -> SocketAddr {
        self.client_addr
    }

    /// Get volume in dB
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Set volume in dB (-144.0 to 0.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(-144.0, 0.0);
        self.touch();
    }

    /// Set stream parameters (from ANNOUNCE)
    pub fn set_stream_params(&mut self, params: StreamParameters) {
        self.stream_params = Some(params);
        self.touch();
    }

    /// Get stream parameters
    pub fn stream_params(&self) -> Option<&StreamParameters> {
        self.stream_params.as_ref()
    }

    /// Set socket configuration (from SETUP)
    pub fn set_sockets(&mut self, sockets: SessionSockets) {
        self.sockets = Some(sockets);
        self.touch();
    }

    /// Get socket configuration
    pub fn sockets(&self) -> Option<&SessionSockets> {
        self.sockets.as_ref()
    }

    /// Set RTSP session ID
    pub fn set_rtsp_session_id(&mut self, id: String) {
        self.rtsp_session_id = Some(id);
    }

    /// Get RTSP session ID
    pub fn rtsp_session_id(&self) -> Option<&str> {
        self.rtsp_session_id.as_deref()
    }

    /// Set initial RTP info (from RECORD)
    pub fn set_rtp_info(&mut self, seq: u16, rtptime: u32) {
        self.initial_seq = Some(seq);
        self.initial_rtptime = Some(rtptime);
        self.touch();
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Get time since last activity
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Get session age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Check if session has timed out
    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.idle_time() > timeout
    }
}

/// Session errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: SessionState,
        to: SessionState,
    },

    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session busy: another session is active")]
    Busy,

    #[error("Session timed out")]
    Timeout,
}

fn generate_session_id() -> String {
    use rand::Rng;
    let id: u64 = rand::thread_rng().gen();
    format!("{:016X}", id)
}
```

---

### 37.2 Session Manager

- [x] **37.2.1** Implement session manager for handling multiple connections

**File:** `src/receiver/session_manager.rs`

```rust
//! Session manager for the receiver
//!
//! Manages session lifecycle, enforces single-session policy,
//! and handles session preemption.

use super::session::{ReceiverSession, SessionState, SessionError, SessionSockets};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};

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
            max_duration: Duration::ZERO,  // Unlimited
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
    SessionStarted { session_id: String, client: SocketAddr },
    /// Session state changed
    StateChanged { session_id: String, new_state: SessionState },
    /// Session ended
    SessionEnded { session_id: String, reason: String },
    /// Volume changed
    VolumeChanged { session_id: String, volume: f32 },
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
    pub audio: UdpSocket,
    pub control: UdpSocket,
    pub timing: UdpSocket,
}

impl AllocatedSockets {
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
        Self { base, range, next: 0 }
    }

    /// Allocate next available port trio
    fn allocate_trio(&mut self) -> (u16, u16, u16) {
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
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    /// Check if a session is currently active
    pub async fn has_active_session(&self) -> bool {
        self.active_session.read().await.is_some()
    }

    /// Get current session info (if any)
    pub async fn current_session_id(&self) -> Option<String> {
        self.active_session.read().await
            .as_ref()
            .map(|s| s.id().to_string())
    }

    /// Start a new session
    pub async fn start_session(
        &self,
        client_addr: SocketAddr,
    ) -> Result<String, SessionError> {
        let mut active = self.active_session.write().await;

        // Check if session already exists
        if let Some(ref existing) = *active {
            match self.config.preemption_policy {
                PreemptionPolicy::Reject => {
                    return Err(SessionError::Busy);
                }
                PreemptionPolicy::AllowPreempt => {
                    // End existing session
                    let old_id = existing.id().to_string();
                    self.cleanup_sockets().await;

                    let _ = self.event_tx.send(SessionEvent::SessionEnded {
                        session_id: old_id,
                        reason: "Preempted by new connection".to_string(),
                    });
                }
                PreemptionPolicy::Queue => {
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
    pub async fn allocate_sockets(&self) -> Result<(u16, u16, u16), std::io::Error> {
        let (audio_port, control_port, timing_port) = {
            let mut allocator = self.port_allocator.lock().await;
            allocator.allocate_trio()
        };

        // Bind sockets
        let audio = UdpSocket::bind(format!("0.0.0.0:{}", audio_port)).await?;
        let control = UdpSocket::bind(format!("0.0.0.0:{}", control_port)).await?;
        let timing = UdpSocket::bind(format!("0.0.0.0:{}", timing_port)).await?;

        let ports = (
            audio.local_addr()?.port(),
            control.local_addr()?.port(),
            timing.local_addr()?.port(),
        );

        let mut sockets = self.sockets.lock().await;
        *sockets = Some(AllocatedSockets { audio, control, timing });

        Ok(ports)
    }

    /// Get reference to allocated sockets
    pub async fn get_sockets(&self) -> Option<Arc<Mutex<Option<AllocatedSockets>>>> {
        // Return clone of Arc for shared access
        Some(self.sockets.clone())
    }

    /// Update session state
    pub async fn update_state(&self, new_state: SessionState) -> Result<(), SessionError> {
        let mut active = self.active_session.write().await;

        let session = active.as_mut()
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
    pub async fn with_session<F, R>(&self, f: F) -> Result<R, SessionError>
    where
        F: FnOnce(&mut ReceiverSession) -> R,
    {
        let mut active = self.active_session.write().await;
        let session = active.as_mut()
            .ok_or_else(|| SessionError::NotFound("No active session".into()))?;
        Ok(f(session))
    }
}
```

---

### 37.3 Timeout Monitor

- [x] **37.3.1** Implement background timeout monitoring

**File:** `src/receiver/session_manager.rs` (continued)

```rust
use tokio::time::interval;

impl SessionManager {
    /// Start background timeout monitor
    ///
    /// Returns a handle that can be used to stop the monitor.
    pub fn start_timeout_monitor(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager = self.clone();
        let check_interval = self.config.idle_timeout / 4;

        tokio::spawn(async move {
            let mut ticker = interval(check_interval);

            loop {
                ticker.tick().await;

                let should_timeout = manager.check_timeout().await;

                if should_timeout {
                    tracing::info!("Session timed out due to inactivity");
                    manager.end_session("Idle timeout").await;
                }

                // Also check max duration if configured
                if manager.config.max_duration > Duration::ZERO {
                    let active = manager.active_session.read().await;
                    if let Some(ref session) = *active {
                        if session.age() > manager.config.max_duration {
                            drop(active);  // Release read lock before write
                            tracing::info!("Session exceeded maximum duration");
                            manager.end_session("Maximum duration exceeded").await;
                        }
                    }
                }
            }
        })
    }
}
```

---

## Unit Tests

### 37.4 Unit Tests

- [x] **37.4.1** Session state machine tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 12345)
    }

    #[test]
    fn test_valid_state_transitions() {
        use SessionState::*;

        assert!(Connected.can_transition_to(Announced));
        assert!(Announced.can_transition_to(Setup));
        assert!(Setup.can_transition_to(Streaming));
        assert!(Streaming.can_transition_to(Paused));
        assert!(Paused.can_transition_to(Streaming));
        assert!(Streaming.can_transition_to(Teardown));
    }

    #[test]
    fn test_invalid_state_transitions() {
        use SessionState::*;

        assert!(!Connected.can_transition_to(Streaming));
        assert!(!Setup.can_transition_to(Connected));
        assert!(!Closed.can_transition_to(Connected));
    }

    #[test]
    fn test_teardown_from_any_state() {
        use SessionState::*;

        for state in [Connected, Announced, Setup, Streaming, Paused] {
            assert!(state.can_transition_to(Teardown),
                    "{:?} should transition to Teardown", state);
        }
    }

    #[test]
    fn test_session_state_change() {
        let mut session = ReceiverSession::new(test_addr());

        assert_eq!(session.state(), SessionState::Connected);

        session.set_state(SessionState::Announced).unwrap();
        assert_eq!(session.state(), SessionState::Announced);

        // Invalid transition should fail
        let result = session.set_state(SessionState::Streaming);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_volume() {
        let mut session = ReceiverSession::new(test_addr());

        assert_eq!(session.volume(), 0.0);

        session.set_volume(-15.0);
        assert_eq!(session.volume(), -15.0);

        // Clamp to valid range
        session.set_volume(-200.0);
        assert_eq!(session.volume(), -144.0);

        session.set_volume(10.0);
        assert_eq!(session.volume(), 0.0);
    }

    #[test]
    fn test_session_timeout() {
        let session = ReceiverSession::new(test_addr());

        // Immediately after creation, should not be timed out
        assert!(!session.is_timed_out(Duration::from_secs(1)));

        // With zero timeout, should be timed out
        assert!(session.is_timed_out(Duration::ZERO));
    }

    #[test]
    fn test_is_active_states() {
        assert!(SessionState::Streaming.is_active());
        assert!(SessionState::Paused.is_active());
        assert!(!SessionState::Connected.is_active());
        assert!(!SessionState::Teardown.is_active());
    }

    #[tokio::test]
    async fn test_session_manager_single_session() {
        let manager = SessionManager::new(SessionManagerConfig::default());

        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

        // Start first session
        let session1 = manager.start_session(addr1).await.unwrap();
        assert!(manager.has_active_session().await);

        // With AllowPreempt policy, second session preempts first
        let session2 = manager.start_session(addr2).await.unwrap();
        assert_ne!(session1, session2);

        // End session
        manager.end_session("test").await;
        assert!(!manager.has_active_session().await);
    }

    #[tokio::test]
    async fn test_session_manager_reject_policy() {
        let config = SessionManagerConfig {
            preemption_policy: PreemptionPolicy::Reject,
            ..Default::default()
        };
        let manager = SessionManager::new(config);

        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

        manager.start_session(addr1).await.unwrap();

        // Second session should be rejected
        let result = manager.start_session(addr2).await;
        assert!(matches!(result, Err(SessionError::Busy)));
    }

    #[tokio::test]
    async fn test_socket_allocation() {
        let manager = SessionManager::new(SessionManagerConfig::default());

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
        manager.start_session(addr).await.unwrap();

        let (audio, control, timing) = manager.allocate_sockets().await.unwrap();

        // Ports should be allocated
        assert!(audio > 0);
        assert!(control > 0);
        assert!(timing > 0);

        // Ports should be sequential (based on allocator)
        assert_eq!(control, audio + 1);
        assert_eq!(timing, audio + 2);
    }
}
```

---

## Integration Tests

### 37.5 Integration Tests

- [x] **37.5.1** Full session lifecycle test

**File:** `tests/receiver/session_tests.rs`

```rust
use airplay2::receiver::session_manager::{
    SessionManager, SessionManagerConfig, SessionEvent, PreemptionPolicy
};
use airplay2::receiver::session::SessionState;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

#[tokio::test]
async fn test_complete_session_lifecycle() {
    let manager = SessionManager::new(SessionManagerConfig::default());
    let mut events = manager.subscribe();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 5000);

    // Start session
    let session_id = manager.start_session(addr).await.unwrap();

    // Verify start event
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionStarted { .. }));

    // Allocate sockets
    let (audio, control, timing) = manager.allocate_sockets().await.unwrap();
    assert!(audio > 0 && control > 0 && timing > 0);

    // Progress through states
    manager.update_state(SessionState::Announced).await.unwrap();
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::StateChanged { new_state: SessionState::Announced, .. }));

    manager.update_state(SessionState::Setup).await.unwrap();
    manager.update_state(SessionState::Streaming).await.unwrap();

    // Set volume
    manager.set_volume(-20.0).await;
    let event = events.recv().await.unwrap();
    let event = events.recv().await.unwrap();
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::VolumeChanged { volume, .. } if (volume - -20.0).abs() < 0.01));

    // End session
    manager.end_session("Test complete").await;
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionEnded { .. }));

    assert!(!manager.has_active_session().await);
}

#[tokio::test]
async fn test_session_preemption() {
    let manager = SessionManager::new(SessionManagerConfig {
        preemption_policy: PreemptionPolicy::AllowPreempt,
        ..Default::default()
    });

    let mut events = manager.subscribe();

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

    // Start first session
    manager.start_session(addr1).await.unwrap();
    let _ = events.recv().await;  // SessionStarted

    // Preempt with second session
    manager.start_session(addr2).await.unwrap();

    // Should get SessionEnded for first, then SessionStarted for second
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionEnded { reason, .. }
        if reason.contains("Preempted")));

    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionStarted { client, .. }
        if client == addr2));
}

#[tokio::test]
async fn test_session_timeout() {
    let config = SessionManagerConfig {
        idle_timeout: Duration::from_millis(100),
        ..Default::default()
    };

    let manager = std::sync::Arc::new(SessionManager::new(config));
    let mut events = manager.subscribe();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    manager.start_session(addr).await.unwrap();
    let _ = events.recv().await;  // SessionStarted

    // Start timeout monitor
    let _monitor = manager.start_timeout_monitor();

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should receive timeout event
    let event = tokio::time::timeout(
        Duration::from_millis(100),
        events.recv()
    ).await.unwrap().unwrap();

    assert!(matches!(event, SessionEvent::SessionEnded { reason, .. }
        if reason.contains("timeout")));
}
```

---

## Acceptance Criteria

- [x] Session state machine validates all transitions
- [x] Invalid transitions return appropriate errors
- [x] Single-session enforcement works with all policies
- [x] UDP socket allocation succeeds and returns valid ports
- [x] Session events broadcast correctly
- [x] Timeout monitoring detects idle sessions
- [x] Session preemption works correctly
- [x] Volume changes tracked per-session
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- **Single session**: AirPlay 1 typically allows only one sender; this is enforced at session level
- **Preemption**: AllowPreempt is friendliest for home use; Reject better for professional setups
- **Socket lifecycle**: Sockets bound on SETUP, released on TEARDOWN or timeout
- **Thread safety**: Uses RwLock for session state, Mutex for sockets
- **Events**: Broadcast channel allows multiple subscribers (UI, logging, etc.)
- **Future**: Queue policy could be implemented for multi-sender scenarios

---

## References

- [RTSP Session Handling](https://tools.ietf.org/html/rfc2326#section-3.4)
- [shairport-sync session management](https://github.com/mikebrady/shairport-sync)
