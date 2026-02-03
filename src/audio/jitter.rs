//! Jitter buffer for RTP packet reordering and timing
//!
//! Buffers incoming packets, reorders them by sequence number,
//! and releases them at the appropriate playback time.

use crate::receiver::rtp_receiver::AudioPacket;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Jitter buffer configuration
#[derive(Debug, Clone)]
pub struct JitterBufferConfig {
    /// Target buffer depth in packets
    pub target_depth: usize,
    /// Minimum depth before playback starts
    pub min_depth: usize,
    /// Maximum depth (excess packets dropped)
    pub max_depth: usize,
    /// Maximum age before packet is considered too old
    pub max_age: Duration,
    /// Packets per second (for timing calculations)
    pub packets_per_second: f64,
}

impl Default for JitterBufferConfig {
    fn default() -> Self {
        Self {
            target_depth: 50, // ~400ms at 352 samples/packet, 44.1kHz
            min_depth: 10,    // ~80ms
            max_depth: 200,   // ~1.6s
            max_age: Duration::from_secs(3),
            packets_per_second: 44100.0 / 352.0, // ~125 packets/sec
        }
    }
}

/// Buffer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    /// Filling up, not ready for playback
    Buffering,
    /// Normal playback
    Playing,
    /// Underrun, rebuffering
    Underrun,
}

/// Statistics from the jitter buffer
#[derive(Debug, Clone, Default)]
pub struct JitterStats {
    /// Total packets received
    pub packets_received: u64,
    /// Packets played successfully
    pub packets_played: u64,
    /// Packets dropped because they were late
    pub packets_dropped_late: u64,
    /// Packets dropped due to buffer overflow
    pub packets_dropped_overflow: u64,
    /// Packets concealed (missing)
    pub packets_concealed: u64,
    /// Number of underrun events
    pub underruns: u64,
    /// Current buffer depth
    pub current_depth: usize,
    /// Maximum depth seen
    pub max_depth_seen: usize,
}

/// Result of adding a packet (kept for backward compatibility if needed, though unused in doc impl)
#[derive(Debug)]
pub enum JitterResult {
    /// Packet buffered
    Buffered,
    /// Packet dropped (late, duplicate, etc)
    Dropped,
}

/// Next packet result (kept for backward compatibility if needed)
#[derive(Debug)]
pub enum NextPacket {
    /// Packet ready
    Ready(AudioPacket),
    /// Wait for more packets
    Wait,
}

/// Buffer health report
#[derive(Debug, Clone)]
pub struct BufferHealth {
    /// Current state
    pub state: BufferState,
    /// Current depth
    pub depth: usize,
    /// Fill ratio (0.0 - 1.0)
    pub fill_ratio: f64,
    /// Estimated latency
    pub estimated_latency: Duration,
    /// Loss rate (0.0 - 1.0)
    pub loss_rate: f64,
    /// Total underruns
    pub underruns: u64,
    /// Whether the buffer is considered healthy
    pub is_healthy: bool,
}

/// Jitter buffer for audio packets
pub struct JitterBuffer {
    config: JitterBufferConfig,
    /// Packets ordered by sequence number
    packets: BTreeMap<u16, AudioPacket>,
    /// Next sequence number expected for playback
    next_play_seq: Option<u16>,
    /// Current state
    state: BufferState,
    /// Statistics
    stats: JitterStats,
    /// Last packet receive time
    #[allow(dead_code)] // Useful for debugging/future use
    last_receive: Option<Instant>,
}

impl JitterBuffer {
    /// Create a new jitter buffer
    #[must_use]
    pub fn new(config: JitterBufferConfig) -> Self {
        Self {
            config,
            packets: BTreeMap::new(),
            next_play_seq: None,
            state: BufferState::Buffering,
            stats: JitterStats::default(),
            last_receive: None,
        }
    }

    /// Insert a packet into the buffer
    pub fn insert(&mut self, packet: AudioPacket) {
        self.stats.packets_received += 1;
        self.last_receive = Some(Instant::now());

        let seq = packet.sequence;

        // Check if packet is too old (behind playback point)
        if let Some(next_seq) = self.next_play_seq {
            #[allow(clippy::cast_possible_wrap)]
            let diff = seq.wrapping_sub(next_seq) as i16;
            if diff < 0 {
                // Packet is late
                self.stats.packets_dropped_late += 1;
                tracing::debug!("Dropping late packet seq={}, expected={}", seq, next_seq);
                return;
            }
        }

        // Check buffer overflow
        if self.packets.len() >= self.config.max_depth {
            self.stats.packets_dropped_overflow += 1;
            // Drop oldest packet
            if let Some(oldest) = self.packets.keys().next().copied() {
                self.packets.remove(&oldest);
            }
        }

        self.packets.insert(seq, packet);

        // Update max depth stat
        if self.packets.len() > self.stats.max_depth_seen {
            self.stats.max_depth_seen = self.packets.len();
        }

        // Check state transitions
        self.update_state();
    }

    /// Get next packet for playback if available
    pub fn pop(&mut self) -> Option<AudioPacket> {
        if self.state == BufferState::Buffering {
            return None;
        }

        let next_seq = self.next_play_seq?;

        if let Some(packet) = self.packets.remove(&next_seq) {
            self.next_play_seq = Some(next_seq.wrapping_add(1));
            self.stats.packets_played += 1;
            self.update_state();
            Some(packet)
        } else {
            // Missing packet - try to conceal or skip
            self.handle_missing_packet(next_seq)
        }
    }

    /// Handle a missing packet at playback time
    fn handle_missing_packet(&mut self, missing_seq: u16) -> Option<AudioPacket> {
        self.stats.packets_concealed += 1;

        // Find the next available packet
        let next_available = self.packets.keys().next().copied();

        if let Some(avail_seq) = next_available {
            let gap = avail_seq.wrapping_sub(missing_seq);

            if gap < 10 {
                // Small gap, skip to available packet
                tracing::debug!(
                    "Concealing gap: {} packets from {} to {}",
                    gap,
                    missing_seq,
                    avail_seq
                );
                self.next_play_seq = Some(avail_seq.wrapping_add(1));
                let packet = self.packets.remove(&avail_seq);
                if packet.is_some() {
                    self.stats.packets_played += 1;
                    self.update_state();
                }
                return packet;
            }
        }

        // Large gap or no packets - advance sequence and return nothing
        self.next_play_seq = Some(missing_seq.wrapping_add(1));
        None
    }

    /// Update buffer state based on current depth
    fn update_state(&mut self) {
        let depth = self.packets.len();

        match self.state {
            BufferState::Buffering => {
                if depth >= self.config.min_depth {
                    self.state = BufferState::Playing;
                    // Set initial playback sequence
                    if self.next_play_seq.is_none() {
                        self.next_play_seq = self.packets.keys().next().copied();
                    }
                    tracing::info!("Jitter buffer ready, starting playback");
                }
            }
            BufferState::Playing => {
                if depth == 0 {
                    self.state = BufferState::Underrun;
                    self.stats.underruns += 1;
                    tracing::warn!("Jitter buffer underrun");
                }
            }
            BufferState::Underrun => {
                if depth >= self.config.min_depth {
                    self.state = BufferState::Playing;
                    tracing::info!("Recovered from underrun");
                }
            }
        }

        self.stats.current_depth = depth;
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> BufferState {
        self.state
    }

    /// Get statistics
    #[must_use]
    pub fn stats(&self) -> &JitterStats {
        &self.stats
    }

    /// Check if buffer is ready for playback
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.state == BufferState::Playing
    }

    /// Get current depth in packets
    #[must_use]
    pub fn depth(&self) -> usize {
        self.packets.len()
    }

    /// Flush all packets (e.g., for FLUSH command)
    pub fn flush(&mut self) {
        self.packets.clear();
        self.state = BufferState::Buffering;
        self.next_play_seq = None;
        tracing::info!("Jitter buffer flushed");
    }

    /// Flush packets from given RTP timestamp onwards
    pub fn flush_from_timestamp(&mut self, rtp_time: u32) {
        // Remove packets with timestamp >= rtp_time
        // This is approximate as we track by sequence, not timestamp
        self.packets.retain(|_, p| p.timestamp < rtp_time);

        if self.packets.is_empty() {
            self.state = BufferState::Buffering;
            self.next_play_seq = None;
        }
    }

    /// Remove old packets
    pub fn prune_old(&mut self) {
        let now = Instant::now();
        let max_age = self.config.max_age;

        self.packets
            .retain(|_, p| now.duration_since(p.received_at) < max_age);
    }

    /// Get fill percentage (0.0 to 1.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn fill_ratio(&self) -> f64 {
        self.packets.len() as f64 / self.config.target_depth as f64
    }

    /// Check if buffer health is good
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        let depth = self.packets.len();
        depth >= self.config.min_depth / 2 && depth <= self.config.max_depth
    }

    /// Get estimated latency based on buffer depth
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn estimated_latency(&self) -> Duration {
        let packets = self.packets.len() as f64;
        Duration::from_secs_f64(packets / self.config.packets_per_second)
    }

    /// Calculate packet loss rate
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn loss_rate(&self) -> f64 {
        let total = self.stats.packets_received + self.stats.packets_concealed;
        if total == 0 {
            return 0.0;
        }
        self.stats.packets_concealed as f64 / total as f64
    }

    /// Get comprehensive health report
    #[must_use]
    pub fn health(&self) -> BufferHealth {
        BufferHealth {
            state: self.state,
            depth: self.packets.len(),
            fill_ratio: self.fill_ratio(),
            estimated_latency: self.estimated_latency(),
            loss_rate: self.loss_rate(),
            underruns: self.stats.underruns,
            is_healthy: self.is_healthy(),
        }
    }
}
