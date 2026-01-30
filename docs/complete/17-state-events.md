# Section 17: State and Events

**VERIFIED**: StateContainer, ClientState, EventBus, event types checked against source. PlaybackState uses struct default.

## Dependencies
- **Section 02**: Core Types (must be complete)
- **Section 10**: Connection Management (must be complete)

## Overview

This section implements a centralized state management system and event bus for:
- Tracking overall client state
- Distributing events to subscribers
- Enabling reactive updates

## Objectives

- Implement state container
- Create event bus with subscriptions
- Support state change notifications
- Enable UI integration patterns

---

## Tasks

### 17.1 State Container

- [x] **17.1.1** Implement state management

**File:** `src/state/container.rs`

```rust
//! Centralized state management

use crate::types::{AirPlayDevice, PlaybackState, TrackInfo};
use crate::control::queue::PlaybackQueue;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

/// Overall client state
#[derive(Debug, Clone)]
pub struct ClientState {
    /// Connected device (if any)
    pub device: Option<AirPlayDevice>,
    /// Current playback state
    pub playback: PlaybackState,
    /// Current track info
    pub current_track: Option<TrackInfo>,
    /// Current volume (0.0 - 1.0)
    pub volume: f32,
    /// Is muted
    pub muted: bool,
    /// Current position (seconds)
    pub position: f64,
    /// Duration (seconds)
    pub duration: f64,
    /// Queue length
    pub queue_length: usize,
    /// Is shuffle enabled
    pub shuffle: bool,
    /// Repeat mode
    pub repeat: RepeatMode,
}

/// Repeat mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            device: None,
            playback: PlaybackState::default(),
            current_track: None,
            volume: 1.0,
            muted: false,
            position: 0.0,
            duration: 0.0,
            queue_length: 0,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }
}

/// State container with change notifications
pub struct StateContainer {
    /// Current state
    state: RwLock<ClientState>,
    /// State change sender
    tx: watch::Sender<ClientState>,
    /// State change receiver (clone for subscribers)
    rx: watch::Receiver<ClientState>,
}

impl StateContainer {
    /// Create a new state container
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(ClientState::default());
        Self {
            state: RwLock::new(ClientState::default()),
            tx,
            rx,
        }
    }

    /// Get current state
    pub async fn get(&self) -> ClientState {
        self.state.read().await.clone()
    }

    /// Subscribe to state changes
    pub fn subscribe(&self) -> watch::Receiver<ClientState> {
        self.rx.clone()
    }

    /// Update state with a function
    pub async fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut ClientState),
    {
        let mut state = self.state.write().await;
        f(&mut state);
        let _ = self.tx.send(state.clone());
    }

    /// Set device
    pub async fn set_device(&self, device: Option<AirPlayDevice>) {
        self.update(|s| s.device = device).await;
    }

    /// Set playback state
    pub async fn set_playback(&self, playback: PlaybackState) {
        self.update(|s| s.playback = playback).await;
    }

    /// Set current track
    pub async fn set_track(&self, track: Option<TrackInfo>) {
        self.update(|s| s.current_track = track).await;
    }

    /// Set volume
    pub async fn set_volume(&self, volume: f32) {
        self.update(|s| {
            s.volume = volume.clamp(0.0, 1.0);
            if s.volume > 0.0 {
                s.muted = false;
            }
        }).await;
    }

    /// Set muted
    pub async fn set_muted(&self, muted: bool) {
        self.update(|s| s.muted = muted).await;
    }

    /// Set position
    pub async fn set_position(&self, position: f64) {
        self.update(|s| s.position = position).await;
    }

    /// Set duration
    pub async fn set_duration(&self, duration: f64) {
        self.update(|s| s.duration = duration).await;
    }

    /// Reset state
    pub async fn reset(&self) {
        self.update(|s| *s = ClientState::default()).await;
    }
}

impl Default for StateContainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_update() {
        let container = StateContainer::new();

        container.set_volume(0.5).await;
        let state = container.get().await;

        assert_eq!(state.volume, 0.5);
    }

    #[tokio::test]
    async fn test_state_subscription() {
        let container = StateContainer::new();
        let mut rx = container.subscribe();

        container.set_volume(0.75).await;

        // Receiver should have the updated state
        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().volume, 0.75);
    }
}
```

---

### 17.2 Event Bus

- [x] **17.2.1** Implement event bus

**File:** `src/state/events.rs`

```rust
//! Event bus for client events

use crate::types::{AirPlayDevice, TrackInfo, PlaybackState};
use tokio::sync::broadcast;
use std::fmt;

/// Client events
#[derive(Debug, Clone)]
pub enum ClientEvent {
    // Connection events
    /// Connected to device
    Connected { device: AirPlayDevice },
    /// Disconnected from device
    Disconnected { device: AirPlayDevice, reason: String },
    /// Connection error
    ConnectionError { message: String },

    // Playback events
    /// Playback state changed
    PlaybackStateChanged { old: PlaybackState, new: PlaybackState },
    /// Track changed
    TrackChanged { track: Option<TrackInfo> },
    /// Position updated
    PositionUpdated { position: f64, duration: f64 },
    /// Seek completed
    SeekCompleted { position: f64 },

    // Volume events
    /// Volume changed
    VolumeChanged { volume: f32 },
    /// Mute state changed
    MuteChanged { muted: bool },

    // Queue events
    /// Queue updated
    QueueUpdated { length: usize },
    /// Track added to queue
    TrackAdded { track: TrackInfo, position: usize },
    /// Track removed from queue
    TrackRemoved { position: usize },

    // Discovery events
    /// Device discovered
    DeviceDiscovered { device: AirPlayDevice },
    /// Device lost
    DeviceLost { device_id: String },

    // Error events
    /// Error occurred
    Error { code: ErrorCode, message: String },
}

/// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Network error
    Network,
    /// Authentication error
    Authentication,
    /// Protocol error
    Protocol,
    /// Playback error
    Playback,
    /// Unknown error
    Unknown,
}

/// Event bus for distributing events
pub struct EventBus {
    /// Broadcast sender
    tx: broadcast::Sender<ClientEvent>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.tx.subscribe()
    }

    /// Emit an event
    pub fn emit(&self, event: ClientEvent) {
        // Ignore error if no receivers
        let _ = self.tx.send(event);
    }

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter for selective subscription
pub struct EventFilter {
    rx: broadcast::Receiver<ClientEvent>,
    filter: Box<dyn Fn(&ClientEvent) -> bool + Send>,
}

impl EventFilter {
    /// Create a filtered event receiver
    pub fn new<F>(bus: &EventBus, filter: F) -> Self
    where
        F: Fn(&ClientEvent) -> bool + Send + 'static,
    {
        Self {
            rx: bus.subscribe(),
            filter: Box::new(filter),
        }
    }

    /// Receive next matching event
    pub async fn recv(&mut self) -> Option<ClientEvent> {
        loop {
            match self.rx.recv().await {
                Ok(event) if (self.filter)(&event) => return Some(event),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Helper functions for common filters
impl EventFilter {
    /// Filter for playback events only
    pub fn playback_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| matches!(e,
            ClientEvent::PlaybackStateChanged { .. } |
            ClientEvent::TrackChanged { .. } |
            ClientEvent::PositionUpdated { .. } |
            ClientEvent::SeekCompleted { .. }
        ))
    }

    /// Filter for connection events only
    pub fn connection_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| matches!(e,
            ClientEvent::Connected { .. } |
            ClientEvent::Disconnected { .. } |
            ClientEvent::ConnectionError { .. }
        ))
    }

    /// Filter for error events only
    pub fn error_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| matches!(e, ClientEvent::Error { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit(ClientEvent::VolumeChanged { volume: 0.5 });

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, ClientEvent::VolumeChanged { volume: 0.5 }));
    }

    #[tokio::test]
    async fn test_event_filter() {
        let bus = EventBus::new();
        let mut filter = EventFilter::playback_events(&bus);

        // Emit non-playback event
        bus.emit(ClientEvent::VolumeChanged { volume: 0.5 });
        // Emit playback event
        bus.emit(ClientEvent::TrackChanged { track: None });

        // Filter should only receive playback event
        let event = filter.recv().await.unwrap();
        assert!(matches!(event, ClientEvent::TrackChanged { .. }));
    }
}
```

---

### 17.3 Module Entry Point

- [x] **17.3.1** Create state module

**File:** `src/state/mod.rs`

```rust
//! State management and events

mod container;
mod events;
#[cfg(test)]
mod tests;

pub use crate::types::RepeatMode;
pub use container::{ClientState, StateContainer};
pub use events::{ClientEvent, ErrorCode, EventBus, EventFilter};
```

---

## Acceptance Criteria

- [x] State container tracks all client state
- [x] State changes notify subscribers
- [x] Event bus distributes events
- [x] Event filtering works
- [x] All unit tests pass

---

## Notes

- Consider adding state persistence
- May need debouncing for rapid position updates
- Event history could be useful for debugging
