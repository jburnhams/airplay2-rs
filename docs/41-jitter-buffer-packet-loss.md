# Section 41: Jitter Buffer & Packet Loss Handling

## Dependencies
- **Section 39**: RTP Receiver Core (audio packets)
- **Section 40**: Timing Synchronization (playback scheduling)
- **Section 12**: Audio Buffer & Timing (ring buffer)

## Overview

This section implements the jitter buffer, which handles:

1. **Packet reordering**: Network may deliver packets out of order
2. **Timing**: Hold packets until their scheduled playback time
3. **Packet loss concealment**: Handle missing packets gracefully
4. **Buffer management**: Maintain appropriate buffer depth

The jitter buffer bridges the gap between unpredictable network arrival times and the steady, continuous audio output required by the audio device.

## Objectives

- Implement sequence-ordered jitter buffer
- Reorder out-of-sequence packets
- Detect and conceal packet loss
- Provide steady packet output stream
- Support configurable buffer depth
- Track buffer statistics (fill level, underruns)

---

## Tasks

### 41.1 Jitter Buffer Core

- [ ] **41.1.1** Implement sequence-keyed jitter buffer

**File:** `src/audio/jitter.rs`

```rust
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
            target_depth: 50,      // ~400ms at 352 samples/packet, 44.1kHz
            min_depth: 10,         // ~80ms
            max_depth: 200,        // ~1.6s
            max_age: Duration::from_secs(3),
            packets_per_second: 44100.0 / 352.0,  // ~125 packets/sec
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
    pub packets_received: u64,
    pub packets_played: u64,
    pub packets_dropped_late: u64,
    pub packets_dropped_overflow: u64,
    pub packets_concealed: u64,
    pub underruns: u64,
    pub current_depth: usize,
    pub max_depth_seen: usize,
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
    last_receive: Option<Instant>,
}

impl JitterBuffer {
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
            let diff = seq.wrapping_sub(next_seq) as i16;
            if diff < 0 && diff > -1000 {
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
            self.stats.current_depth = self.packets.len();
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
                tracing::debug!("Concealing gap: {} packets from {} to {}", gap, missing_seq, avail_seq);
                self.next_play_seq = Some(avail_seq.wrapping_add(1));
                return self.packets.remove(&avail_seq);
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
    pub fn state(&self) -> BufferState {
        self.state
    }

    /// Get statistics
    pub fn stats(&self) -> &JitterStats {
        &self.stats
    }

    /// Check if buffer is ready for playback
    pub fn is_ready(&self) -> bool {
        self.state == BufferState::Playing
    }

    /// Get current depth in packets
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

        self.packets.retain(|_, p| {
            now.duration_since(p.received_at) < max_age
        });
    }
}
```

---

### 41.2 Packet Loss Concealment

- [ ] **41.2.1** Implement basic concealment strategies

**File:** `src/audio/concealment.rs`

```rust
//! Packet loss concealment strategies
//!
//! When packets are lost, we need to fill the gap to maintain
//! continuous audio output.

/// Concealment strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcealmentStrategy {
    /// Fill with silence (zeros)
    Silence,
    /// Repeat the previous packet
    Repeat,
    /// Fade to silence over the gap
    FadeOut,
    /// Interpolate if next packet arrives
    Interpolate,
}

impl Default for ConcealmentStrategy {
    fn default() -> Self {
        ConcealmentStrategy::Repeat
    }
}

/// Concealment generator
pub struct Concealer {
    strategy: ConcealmentStrategy,
    /// Previous packet audio data (for repeat)
    previous_audio: Option<Vec<u8>>,
    /// Sample rate
    sample_rate: u32,
    /// Bytes per sample (e.g., 4 for 16-bit stereo)
    bytes_per_sample: usize,
}

impl Concealer {
    pub fn new(strategy: ConcealmentStrategy, sample_rate: u32, bytes_per_sample: usize) -> Self {
        Self {
            strategy,
            previous_audio: None,
            sample_rate,
            bytes_per_sample,
        }
    }

    /// Record a good packet for later concealment
    pub fn record_good_packet(&mut self, audio: &[u8]) {
        self.previous_audio = Some(audio.to_vec());
    }

    /// Generate concealment audio for a missing packet
    pub fn conceal(&self, packet_samples: usize) -> Vec<u8> {
        let size = packet_samples * self.bytes_per_sample;

        match self.strategy {
            ConcealmentStrategy::Silence => {
                vec![0u8; size]
            }
            ConcealmentStrategy::Repeat => {
                self.previous_audio
                    .clone()
                    .unwrap_or_else(|| vec![0u8; size])
            }
            ConcealmentStrategy::FadeOut => {
                // Fade previous packet to silence
                if let Some(ref prev) = self.previous_audio {
                    self.fade_out(prev, size)
                } else {
                    vec![0u8; size]
                }
            }
            ConcealmentStrategy::Interpolate => {
                // Would need next packet; fall back to repeat
                self.previous_audio
                    .clone()
                    .unwrap_or_else(|| vec![0u8; size])
            }
        }
    }

    /// Fade audio to silence
    fn fade_out(&self, audio: &[u8], target_size: usize) -> Vec<u8> {
        let mut output = audio.to_vec();
        output.resize(target_size, 0);

        // Simple linear fade for 16-bit samples
        let sample_count = output.len() / 2;
        for i in 0..sample_count {
            let fade = 1.0 - (i as f32 / sample_count as f32);

            let idx = i * 2;
            if idx + 1 < output.len() {
                let sample = i16::from_le_bytes([output[idx], output[idx + 1]]);
                let faded = (sample as f32 * fade) as i16;
                let bytes = faded.to_le_bytes();
                output[idx] = bytes[0];
                output[idx + 1] = bytes[1];
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_concealment() {
        let concealer = Concealer::new(ConcealmentStrategy::Silence, 44100, 4);
        let concealed = concealer.conceal(352);

        assert_eq!(concealed.len(), 352 * 4);
        assert!(concealed.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_repeat_concealment() {
        let mut concealer = Concealer::new(ConcealmentStrategy::Repeat, 44100, 4);

        let audio = vec![0xAB; 1408];  // 352 samples * 4 bytes
        concealer.record_good_packet(&audio);

        let concealed = concealer.conceal(352);
        assert_eq!(concealed, audio);
    }

    #[test]
    fn test_repeat_no_previous() {
        let concealer = Concealer::new(ConcealmentStrategy::Repeat, 44100, 4);
        let concealed = concealer.conceal(352);

        assert_eq!(concealed.len(), 352 * 4);
        assert!(concealed.iter().all(|&b| b == 0));
    }
}
```

---

### 41.3 Buffer Statistics & Monitoring

- [ ] **41.3.1** Implement buffer health monitoring

**File:** `src/audio/jitter.rs` (additions)

```rust
impl JitterBuffer {
    /// Get fill percentage (0.0 to 1.0)
    pub fn fill_ratio(&self) -> f64 {
        self.packets.len() as f64 / self.config.target_depth as f64
    }

    /// Check if buffer health is good
    pub fn is_healthy(&self) -> bool {
        let depth = self.packets.len();
        depth >= self.config.min_depth / 2 && depth <= self.config.max_depth
    }

    /// Get estimated latency based on buffer depth
    pub fn estimated_latency(&self) -> Duration {
        let packets = self.packets.len() as f64;
        Duration::from_secs_f64(packets / self.config.packets_per_second)
    }

    /// Calculate packet loss rate
    pub fn loss_rate(&self) -> f64 {
        let total = self.stats.packets_received + self.stats.packets_concealed;
        if total == 0 {
            return 0.0;
        }
        self.stats.packets_concealed as f64 / total as f64
    }
}

/// Buffer health report
#[derive(Debug, Clone)]
pub struct BufferHealth {
    pub state: BufferState,
    pub depth: usize,
    pub fill_ratio: f64,
    pub estimated_latency: Duration,
    pub loss_rate: f64,
    pub underruns: u64,
    pub is_healthy: bool,
}

impl JitterBuffer {
    /// Get comprehensive health report
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
```

---

## Unit Tests

### 41.4 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(seq: u16, timestamp: u32) -> AudioPacket {
        AudioPacket {
            sequence: seq,
            timestamp,
            ssrc: 0x12345678,
            audio_data: vec![0u8; 1408],
            received_at: Instant::now(),
        }
    }

    #[test]
    fn test_in_order_packets() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 3,
            ..Default::default()
        });

        buffer.insert(make_packet(1, 352));
        buffer.insert(make_packet(2, 704));
        buffer.insert(make_packet(3, 1056));

        assert!(buffer.is_ready());
        assert_eq!(buffer.depth(), 3);

        let p1 = buffer.pop().unwrap();
        assert_eq!(p1.sequence, 1);

        let p2 = buffer.pop().unwrap();
        assert_eq!(p2.sequence, 2);
    }

    #[test]
    fn test_out_of_order_packets() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 3,
            ..Default::default()
        });

        // Insert out of order
        buffer.insert(make_packet(3, 1056));
        buffer.insert(make_packet(1, 352));
        buffer.insert(make_packet(2, 704));

        assert!(buffer.is_ready());

        // Should pop in order
        assert_eq!(buffer.pop().unwrap().sequence, 1);
        assert_eq!(buffer.pop().unwrap().sequence, 2);
        assert_eq!(buffer.pop().unwrap().sequence, 3);
    }

    #[test]
    fn test_buffering_state() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 3,
            ..Default::default()
        });

        buffer.insert(make_packet(1, 352));
        assert!(!buffer.is_ready());

        buffer.insert(make_packet(2, 704));
        assert!(!buffer.is_ready());

        buffer.insert(make_packet(3, 1056));
        assert!(buffer.is_ready());
    }

    #[test]
    fn test_late_packet_dropped() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 2,
            ..Default::default()
        });

        buffer.insert(make_packet(10, 3520));
        buffer.insert(make_packet(11, 3872));

        // Pop first packet
        buffer.pop();

        // Now insert a late packet (seq 5, before current playback)
        buffer.insert(make_packet(5, 1760));

        assert_eq!(buffer.stats.packets_dropped_late, 1);
    }

    #[test]
    fn test_flush() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 2,
            ..Default::default()
        });

        buffer.insert(make_packet(1, 352));
        buffer.insert(make_packet(2, 704));
        buffer.insert(make_packet(3, 1056));

        buffer.flush();

        assert_eq!(buffer.depth(), 0);
        assert_eq!(buffer.state(), BufferState::Buffering);
    }

    #[test]
    fn test_underrun() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 2,
            ..Default::default()
        });

        buffer.insert(make_packet(1, 352));
        buffer.insert(make_packet(2, 704));

        buffer.pop();
        buffer.pop();

        // Buffer now empty
        let result = buffer.pop();
        assert!(result.is_none());
        assert_eq!(buffer.state(), BufferState::Underrun);
    }

    #[test]
    fn test_wraparound_sequence() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig {
            min_depth: 2,
            ..Default::default()
        });

        buffer.insert(make_packet(65534, 0));
        buffer.insert(make_packet(65535, 352));
        buffer.insert(make_packet(0, 704));

        assert_eq!(buffer.pop().unwrap().sequence, 65534);
        assert_eq!(buffer.pop().unwrap().sequence, 65535);
        assert_eq!(buffer.pop().unwrap().sequence, 0);
    }
}
```

---

## Acceptance Criteria

- [ ] Buffer receives and stores packets by sequence
- [ ] Out-of-order packets reordered correctly
- [ ] Late packets (behind playback) dropped
- [ ] Buffer overflow handled (oldest dropped)
- [ ] Buffering state transitions correctly
- [ ] Underrun detected and recovered
- [ ] Concealment generates appropriate fill audio
- [ ] Flush clears buffer and resets state
- [ ] Statistics track all relevant metrics
- [ ] 16-bit sequence wraparound handled
- [ ] All unit tests pass

---

## Notes

- **BTreeMap**: Used for efficient ordered iteration by sequence
- **Concealment**: Simple strategies; could add more sophisticated PLC
- **Depth tuning**: Trades latency for robustness
- **Underrun recovery**: Waits for min_depth before resuming
- **Memory**: Each packet buffered (~1.5KB), max_depth limits memory

---

## References

- [Jitter Buffer Design](https://en.wikipedia.org/wiki/Jitter_buffer)
- [WebRTC Jitter Buffer](https://webrtc.googlesource.com/src/+/refs/heads/main/modules/audio_coding/neteq/)
