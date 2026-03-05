//! Event bus for client events

use tokio::sync::broadcast;

use crate::types::{AirPlayDevice, PlaybackState, TrackInfo};

/// Client events
#[derive(Debug, Clone)]
pub enum ClientEvent {
    // Connection events
    /// Connected to device
    Connected {
        /// The connected device
        device: AirPlayDevice,
    },
    /// Disconnected from device
    Disconnected {
        /// The disconnected device
        device: AirPlayDevice,
        /// Reason for disconnection
        reason: String,
    },
    /// Connection error
    ConnectionError {
        /// Error message
        message: String,
    },

    // Playback events
    /// Playback state changed
    PlaybackStateChanged {
        /// Old state
        old: Box<PlaybackState>,
        /// New state
        new: Box<PlaybackState>,
    },
    /// Track changed
    TrackChanged {
        /// New track info
        track: Option<TrackInfo>,
    },
    /// Position updated
    PositionUpdated {
        /// New position
        position: f64,
        /// Duration
        duration: f64,
    },
    /// Seek completed
    SeekCompleted {
        /// New position
        position: f64,
    },

    // Volume events
    /// Volume changed
    VolumeChanged {
        /// New volume level
        volume: f32,
    },
    /// Mute state changed
    MuteChanged {
        /// New mute state
        muted: bool,
    },

    // Queue events
    /// Queue updated
    QueueUpdated {
        /// New queue length
        length: usize,
    },
    /// Track added to queue
    TrackAdded {
        /// Added track
        track: TrackInfo,
        /// Position in queue
        position: usize,
    },
    /// Track removed from queue
    TrackRemoved {
        /// Position in queue
        position: usize,
    },

    // Discovery events
    /// Device discovered
    DeviceDiscovered {
        /// Discovered device
        device: AirPlayDevice,
    },
    /// Device lost
    DeviceLost {
        /// ID of lost device
        device_id: String,
    },

    // Error events
    /// Error occurred
    Error {
        /// Error code
        code: ErrorCode,
        /// Error message
        message: String,
    },
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
    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    /// Subscribe to events
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.tx.subscribe()
    }

    /// Emit an event
    pub fn emit(&self, event: ClientEvent) {
        // Ignore error if no receivers
        let _ = self.tx.send(event);
    }

    /// Get subscriber count
    #[must_use]
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
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Helper functions for common filters
impl EventFilter {
    /// Filter for playback events only
    #[must_use]
    pub fn playback_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| {
            matches!(
                e,
                ClientEvent::PlaybackStateChanged { .. }
                    | ClientEvent::TrackChanged { .. }
                    | ClientEvent::PositionUpdated { .. }
                    | ClientEvent::SeekCompleted { .. }
            )
        })
    }

    /// Filter for connection events only
    #[must_use]
    pub fn connection_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| {
            matches!(
                e,
                ClientEvent::Connected { .. }
                    | ClientEvent::Disconnected { .. }
                    | ClientEvent::ConnectionError { .. }
            )
        })
    }

    /// Filter for error events only
    #[must_use]
    pub fn error_events(bus: &EventBus) -> Self {
        Self::new(bus, |e| matches!(e, ClientEvent::Error { .. }))
    }
}
