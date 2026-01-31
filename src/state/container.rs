//! Centralized state management

use crate::types::{AirPlayDevice, PlaybackState, RepeatMode, TrackInfo};
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

impl Default for ClientState {
    fn default() -> Self {
        Self {
            device: None,
            playback: PlaybackState::default(),
            current_track: None,
            volume: 0.75, // Match Volume::DEFAULT
            muted: false,
            position: 0.0,
            duration: 0.0,
            queue_length: 0,
            shuffle: false,
            repeat: RepeatMode::default(),
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
    #[must_use]
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
        })
        .await;
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
