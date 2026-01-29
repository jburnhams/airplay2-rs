# Section 56: Buffering & Jitter Management

## Dependencies
- **Section 54**: RTP Audio Receiver
- **Section 55**: PTP Timing Synchronization
- **Section 41**: Jitter Buffering (AirPlay 1 patterns)

## Overview

The jitter buffer absorbs timing variations in packet arrival, reorders out-of-order packets, and provides smooth audio output. AirPlay 2's buffered audio feature (bit 38) enables larger buffers for multi-room synchronization.

## Objectives

- Implement adaptive jitter buffer
- Handle packet reordering by sequence number
- Detect and conceal packet loss
- Support configurable buffer depth for multi-room
- Provide audio samples at precise playback times

---

## Tasks

### 56.1 Jitter Buffer Implementation

**File:** `src/receiver/ap2/jitter_buffer.rs`

```rust
//! Adaptive Jitter Buffer for AirPlay 2
//!
//! Buffers audio frames to handle network jitter and enable
//! synchronized multi-room playback.

use super::rtp_receiver::AudioFrame;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Jitter buffer configuration
#[derive(Debug, Clone)]
pub struct JitterBufferConfig {
    /// Minimum buffer depth (ms)
    pub min_depth_ms: u32,
    /// Maximum buffer depth (ms)
    pub max_depth_ms: u32,
    /// Target buffer depth (ms)
    pub target_depth_ms: u32,
    /// Sample rate
    pub sample_rate: u32,
    /// Channels
    pub channels: u8,
}

impl Default for JitterBufferConfig {
    fn default() -> Self {
        Self {
            min_depth_ms: 50,
            max_depth_ms: 2000,
            target_depth_ms: 200,
            sample_rate: 44100,
            channels: 2,
        }
    }
}

/// Adaptive jitter buffer
pub struct JitterBuffer {
    config: JitterBufferConfig,
    /// Frames indexed by RTP timestamp
    frames: BTreeMap<u32, AudioFrame>,
    /// Current playback position (RTP timestamp)
    playback_position: u32,
    /// Buffer state
    state: BufferState,
    /// Statistics
    stats: BufferStats,
    /// Last sequence seen
    last_sequence: Option<u16>,
    /// Playback started flag
    started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    /// Initial buffering
    Buffering,
    /// Normal playback
    Playing,
    /// Underrun - need more data
    Underrun,
    /// Overflow - discarding old data
    Overflow,
}

#[derive(Debug, Default)]
pub struct BufferStats {
    pub frames_received: u64,
    pub frames_played: u64,
    pub frames_dropped: u64,
    pub frames_lost: u64,
    pub underruns: u64,
    pub overflows: u64,
    pub current_depth_ms: u32,
    pub jitter_estimate_ms: f32,
}

impl JitterBuffer {
    pub fn new(config: JitterBufferConfig) -> Self {
        Self {
            config,
            frames: BTreeMap::new(),
            playback_position: 0,
            state: BufferState::Buffering,
            stats: BufferStats::default(),
            last_sequence: None,
            started: false,
        }
    }

    /// Add a frame to the buffer
    pub fn push(&mut self, frame: AudioFrame) {
        self.stats.frames_received += 1;

        // Check for sequence gaps
        if let Some(last_seq) = self.last_sequence {
            let expected = last_seq.wrapping_add(1);
            if frame.sequence != expected {
                let gap = frame.sequence.wrapping_sub(expected);
                if gap < 100 {  // Reasonable gap
                    self.stats.frames_lost += gap as u64;
                }
            }
        }
        self.last_sequence = Some(frame.sequence);

        // Insert frame
        self.frames.insert(frame.timestamp, frame);

        // Update buffer depth
        self.update_depth();

        // Check for overflow
        if self.stats.current_depth_ms > self.config.max_depth_ms {
            self.handle_overflow();
        }

        // Transition from buffering to playing
        if self.state == BufferState::Buffering
            && self.stats.current_depth_ms >= self.config.target_depth_ms
        {
            self.state = BufferState::Playing;
            self.started = true;
            log::info!("Jitter buffer: starting playback at {}ms depth",
                self.stats.current_depth_ms);
        }
    }

    /// Get samples for playback
    ///
    /// Returns `sample_count` samples, applying loss concealment if needed.
    pub fn pull(&mut self, sample_count: usize) -> Vec<i16> {
        if self.state == BufferState::Buffering {
            // Return silence while buffering
            return vec![0i16; sample_count * self.config.channels as usize];
        }

        let mut output = Vec::with_capacity(sample_count * self.config.channels as usize);

        // Calculate how many RTP timestamps we need
        let timestamps_needed = sample_count as u32;

        while output.len() < sample_count * self.config.channels as usize {
            // Look for frame at playback position
            if let Some(frame) = self.frames.remove(&self.playback_position) {
                // Use real samples
                let needed = (sample_count * self.config.channels as usize) - output.len();
                let available = frame.samples.len().min(needed);
                output.extend_from_slice(&frame.samples[..available]);
                self.stats.frames_played += 1;
            } else {
                // Missing frame - loss concealment
                self.stats.frames_lost += 1;
                // Insert silence or interpolated samples
                let silence_samples = 352 * self.config.channels as usize;  // One frame
                output.extend(vec![0i16; silence_samples.min(
                    sample_count * self.config.channels as usize - output.len()
                )]);
            }

            // Advance playback position (352 samples per frame typical)
            self.playback_position = self.playback_position.wrapping_add(352);
        }

        // Check for underrun
        if self.frames.is_empty() && self.started {
            self.state = BufferState::Underrun;
            self.stats.underruns += 1;
            log::warn!("Jitter buffer underrun");
        }

        self.update_depth();
        output
    }

    /// Flush buffer and reset to initial state
    pub fn flush(&mut self) {
        self.frames.clear();
        self.state = BufferState::Buffering;
        self.started = false;
        log::debug!("Jitter buffer flushed");
    }

    /// Flush to specific RTP timestamp
    pub fn flush_to(&mut self, timestamp: u32) {
        self.frames.retain(|&ts, _| ts >= timestamp);
        self.playback_position = timestamp;
        log::debug!("Jitter buffer flushed to timestamp {}", timestamp);
    }

    /// Set playback position for synchronized start
    pub fn set_playback_position(&mut self, timestamp: u32) {
        self.playback_position = timestamp;
    }

    fn update_depth(&mut self) {
        if self.frames.is_empty() {
            self.stats.current_depth_ms = 0;
            return;
        }

        let first = *self.frames.keys().next().unwrap();
        let last = *self.frames.keys().last().unwrap();

        let depth_samples = last.wrapping_sub(first);
        self.stats.current_depth_ms =
            (depth_samples as u64 * 1000 / self.config.sample_rate as u64) as u32;
    }

    fn handle_overflow(&mut self) {
        self.stats.overflows += 1;
        log::warn!("Jitter buffer overflow at {}ms", self.stats.current_depth_ms);

        // Remove oldest frames until at target
        while self.stats.current_depth_ms > self.config.target_depth_ms {
            if let Some((&ts, _)) = self.frames.iter().next() {
                self.frames.remove(&ts);
                self.stats.frames_dropped += 1;
            } else {
                break;
            }
            self.update_depth();
        }

        self.state = BufferState::Playing;
    }

    pub fn state(&self) -> BufferState {
        self.state
    }

    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }

    pub fn depth_ms(&self) -> u32 {
        self.stats.current_depth_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(seq: u16, ts: u32) -> AudioFrame {
        AudioFrame {
            sequence: seq,
            timestamp: ts,
            samples: vec![0i16; 704],  // 352 stereo samples
            receive_time: Instant::now(),
        }
    }

    #[test]
    fn test_buffering_to_playing() {
        let config = JitterBufferConfig {
            target_depth_ms: 100,
            sample_rate: 44100,
            ..Default::default()
        };
        let mut buffer = JitterBuffer::new(config);

        // Add frames until target depth reached
        for i in 0..20 {
            buffer.push(make_frame(i, i as u32 * 352));
        }

        assert_eq!(buffer.state(), BufferState::Playing);
    }

    #[test]
    fn test_sequence_gap_detection() {
        let mut buffer = JitterBuffer::new(JitterBufferConfig::default());

        buffer.push(make_frame(1, 352));
        buffer.push(make_frame(5, 352 * 5));  // Gap of 3 frames

        assert_eq!(buffer.stats().frames_lost, 3);
    }
}
```

---

## Acceptance Criteria

- [ ] Frames buffered by RTP timestamp
- [ ] Smooth transition from buffering to playing
- [ ] Loss concealment for missing frames
- [ ] Overflow handling with oldest frame removal
- [ ] Underrun detection
- [ ] Flush support for seek/stop
- [ ] Statistics tracking
- [ ] All unit tests pass

---

## References

- [Jitter Buffer Design](https://en.wikipedia.org/wiki/Jitter_buffer)
- [Section 41: Jitter Buffering](./complete/41-jitter-buffering.md)
