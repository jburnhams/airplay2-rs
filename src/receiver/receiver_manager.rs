//! Combined RTP receiver manager
//!
//! Manages all three UDP receive loops and coordinates
//! packet flow to the audio pipeline.

use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;

use super::control_receiver::{ControlEvent, ControlReceiver};
use super::rtp_receiver::{AudioPacket, RtpAudioReceiver};
use super::sequence_tracker::SequenceTracker;
use crate::receiver::session::StreamParameters;

/// Receiver manager configuration
#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    /// Audio packet channel buffer size
    pub audio_buffer_size: usize,
    /// Control event channel buffer size
    pub control_buffer_size: usize,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            audio_buffer_size: 512,
            control_buffer_size: 64,
        }
    }
}

/// Manages all RTP receive operations
pub struct ReceiverManager {
    #[allow(dead_code)]
    config: ReceiverConfig,
    audio_rx: mpsc::Receiver<AudioPacket>,
    control_rx: mpsc::Receiver<ControlEvent>,
    sequence_tracker: Arc<RwLock<SequenceTracker>>,
    handles: Vec<JoinHandle<()>>,
}

impl ReceiverManager {
    /// Start receivers on provided sockets
    #[must_use]
    pub fn start(
        audio_socket: Arc<UdpSocket>,
        control_socket: Arc<UdpSocket>,
        stream_params: StreamParameters,
        config: ReceiverConfig,
    ) -> Self {
        let (audio_tx, audio_rx) = mpsc::channel(config.audio_buffer_size);
        let (control_tx, control_rx) = mpsc::channel(config.control_buffer_size);
        let sequence_tracker = Arc::new(RwLock::new(SequenceTracker::new()));

        // Start audio receiver
        let audio_receiver = RtpAudioReceiver::new(audio_socket, stream_params, audio_tx);

        let audio_handle = tokio::spawn(async move {
            if let Err(e) = audio_receiver.run().await {
                tracing::error!("Audio receiver error: {}", e);
            }
        });

        // Start control receiver
        let control_receiver = ControlReceiver::new(control_socket, control_tx);

        let control_handle = tokio::spawn(async move {
            if let Err(e) = control_receiver.run().await {
                tracing::error!("Control receiver error: {}", e);
            }
        });

        Self {
            config,
            audio_rx,
            control_rx,
            sequence_tracker,
            handles: vec![audio_handle, control_handle],
        }
    }

    /// Receive next audio packet
    pub async fn recv_audio(&mut self) -> Option<AudioPacket> {
        let packet = self.audio_rx.recv().await?;

        // Track sequence
        let mut tracker = self.sequence_tracker.write().await;
        if let Some(gap) = tracker.record(packet.sequence) {
            tracing::debug!(
                "Packet loss detected: {} packets starting at seq {}",
                gap.count,
                gap.start
            );
        }

        Some(packet)
    }

    /// Receive next control event
    pub async fn recv_control(&mut self) -> Option<ControlEvent> {
        self.control_rx.recv().await
    }

    /// Get sequence tracker for statistics
    #[must_use]
    pub fn sequence_tracker(&self) -> Arc<RwLock<SequenceTracker>> {
        self.sequence_tracker.clone()
    }

    /// Stop all receivers
    pub fn stop(self) {
        for handle in self.handles {
            handle.abort();
        }
    }
}
