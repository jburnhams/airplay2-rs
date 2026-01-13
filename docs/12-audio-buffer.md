# Section 12: Audio Buffer and Timing

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 06**: RTP Protocol (for timing references)
- **Section 11**: Audio Formats (must be complete)

## Overview

This section implements audio buffering and timing synchronization for AirPlay streaming. Proper buffering ensures:
- Smooth playback without dropouts
- Accurate timing synchronization
- Handling of network jitter

## Objectives

- Implement audio ring buffer
- Implement jitter buffer for network compensation
- Provide timing/clock synchronization
- Support buffered and realtime modes

---

## Tasks

### 12.1 Ring Buffer

- [ ] **12.1.1** Implement lock-free ring buffer

**File:** `src/audio/buffer.rs`

```rust
//! Audio ring buffer implementation

use std::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free ring buffer for audio samples
pub struct AudioRingBuffer {
    /// Buffer storage
    data: Vec<u8>,
    /// Buffer capacity in bytes
    capacity: usize,
    /// Read position
    read_pos: AtomicUsize,
    /// Write position
    write_pos: AtomicUsize,
    /// High watermark for buffering
    high_watermark: usize,
    /// Low watermark (trigger underrun warning)
    low_watermark: usize,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            capacity,
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
            high_watermark: capacity * 3 / 4,
            low_watermark: capacity / 4,
        }
    }

    /// Create with custom watermarks
    pub fn with_watermarks(capacity: usize, low: usize, high: usize) -> Self {
        assert!(low < high && high <= capacity);
        Self {
            data: vec![0u8; capacity],
            capacity,
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
            high_watermark: high,
            low_watermark: low,
        }
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get current fill level
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);

        if write >= read {
            write - read
        } else {
            self.capacity - read + write
        }
    }

    /// Get free space
    pub fn free(&self) -> usize {
        self.capacity - self.available() - 1
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.free() == 0
    }

    /// Check if below low watermark
    pub fn is_underrunning(&self) -> bool {
        self.available() < self.low_watermark
    }

    /// Check if above high watermark
    pub fn is_ready(&self) -> bool {
        self.available() >= self.high_watermark
    }

    /// Write data to buffer
    ///
    /// Returns number of bytes written
    pub fn write(&self, data: &[u8]) -> usize {
        let available_space = self.free();
        let to_write = data.len().min(available_space);

        if to_write == 0 {
            return 0;
        }

        let write_pos = self.write_pos.load(Ordering::Acquire);

        // Safety: we're the only writer (assumed single-producer)
        let data_ptr = self.data.as_ptr() as *mut u8;

        let first_part = (self.capacity - write_pos).min(to_write);
        let second_part = to_write - first_part;

        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                data_ptr.add(write_pos),
                first_part,
            );

            if second_part > 0 {
                std::ptr::copy_nonoverlapping(
                    data.as_ptr().add(first_part),
                    data_ptr,
                    second_part,
                );
            }
        }

        let new_write_pos = (write_pos + to_write) % self.capacity;
        self.write_pos.store(new_write_pos, Ordering::Release);

        to_write
    }

    /// Read data from buffer
    ///
    /// Returns number of bytes read
    pub fn read(&self, output: &mut [u8]) -> usize {
        let available = self.available();
        let to_read = output.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        let first_part = (self.capacity - read_pos).min(to_read);
        let second_part = to_read - first_part;

        output[..first_part].copy_from_slice(&self.data[read_pos..read_pos + first_part]);

        if second_part > 0 {
            output[first_part..to_read].copy_from_slice(&self.data[..second_part]);
        }

        let new_read_pos = (read_pos + to_read) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_read
    }

    /// Peek at data without consuming
    pub fn peek(&self, output: &mut [u8]) -> usize {
        let available = self.available();
        let to_peek = output.len().min(available);

        if to_peek == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        let first_part = (self.capacity - read_pos).min(to_peek);
        let second_part = to_peek - first_part;

        output[..first_part].copy_from_slice(&self.data[read_pos..read_pos + first_part]);

        if second_part > 0 {
            output[first_part..to_peek].copy_from_slice(&self.data[..second_part]);
        }

        to_peek
    }

    /// Skip/discard bytes from read position
    pub fn skip(&self, count: usize) -> usize {
        let available = self.available();
        let to_skip = count.min(available);

        let read_pos = self.read_pos.load(Ordering::Acquire);
        let new_read_pos = (read_pos + to_skip) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_skip
    }

    /// Clear the buffer
    pub fn clear(&self) {
        self.read_pos.store(0, Ordering::Release);
        self.write_pos.store(0, Ordering::Release);
    }
}

// Thread safety
unsafe impl Send for AudioRingBuffer {}
unsafe impl Sync for AudioRingBuffer {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read_simple() {
        let buffer = AudioRingBuffer::new(1024);

        let data = vec![1u8, 2, 3, 4, 5];
        let written = buffer.write(&data);
        assert_eq!(written, 5);
        assert_eq!(buffer.available(), 5);

        let mut output = vec![0u8; 5];
        let read = buffer.read(&mut output);
        assert_eq!(read, 5);
        assert_eq!(output, data);
    }

    #[test]
    fn test_wraparound() {
        let buffer = AudioRingBuffer::new(8);

        // Write 5 bytes
        buffer.write(&[1, 2, 3, 4, 5]);
        // Read 3 bytes
        let mut out = vec![0u8; 3];
        buffer.read(&mut out);
        assert_eq!(out, vec![1, 2, 3]);

        // Write 5 more (should wrap)
        buffer.write(&[6, 7, 8, 9, 10]);

        // Read all
        let mut out = vec![0u8; 7];
        let n = buffer.read(&mut out);
        assert_eq!(n, 7);
        assert_eq!(out, vec![4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_peek() {
        let buffer = AudioRingBuffer::new(1024);
        buffer.write(&[1, 2, 3, 4, 5]);

        let mut out = vec![0u8; 3];
        let peeked = buffer.peek(&mut out);
        assert_eq!(peeked, 3);
        assert_eq!(out, vec![1, 2, 3]);

        // Data should still be there
        assert_eq!(buffer.available(), 5);
    }
}
```

---

### 12.2 Jitter Buffer

- [ ] **12.2.1** Implement jitter buffer for network compensation

**File:** `src/audio/jitter.rs`

```rust
//! Jitter buffer for handling network timing variations

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Jitter buffer for reordering and timing RTP packets
pub struct JitterBuffer<T> {
    /// Buffered packets by sequence number
    packets: BTreeMap<u16, PacketEntry<T>>,
    /// Expected next sequence number
    next_seq: u16,
    /// Target buffer depth in packets
    target_depth: usize,
    /// Maximum buffer size
    max_size: usize,
    /// Late packet threshold
    late_threshold: u16,
    /// Statistics
    stats: JitterStats,
}

struct PacketEntry<T> {
    packet: T,
    received_at: Instant,
}

/// Jitter buffer statistics
#[derive(Debug, Clone, Default)]
pub struct JitterStats {
    /// Total packets received
    pub packets_received: u64,
    /// Packets dropped (too late)
    pub packets_late: u64,
    /// Packets dropped (duplicate)
    pub packets_duplicate: u64,
    /// Packets dropped (buffer overflow)
    pub packets_overflow: u64,
    /// Current buffer depth
    pub current_depth: usize,
    /// Average jitter in milliseconds
    pub avg_jitter_ms: f64,
}

/// Result of adding a packet
#[derive(Debug)]
pub enum JitterResult<T> {
    /// Packet buffered successfully
    Buffered,
    /// Packet was too late (already played)
    TooLate,
    /// Packet was a duplicate
    Duplicate,
    /// Buffer overflow, oldest packet returned
    Overflow(T),
}

/// Result of getting next packet
#[derive(Debug)]
pub enum NextPacket<T> {
    /// Packet ready
    Ready(T),
    /// Need to wait (not enough buffered)
    Wait,
    /// Gap detected (missing packet)
    Gap { expected: u16, available: u16 },
}

impl<T> JitterBuffer<T> {
    /// Create a new jitter buffer
    pub fn new(target_depth: usize, max_size: usize) -> Self {
        Self {
            packets: BTreeMap::new(),
            next_seq: 0,
            target_depth,
            max_size,
            late_threshold: 100, // ~100 packets late is definitely too late
            stats: JitterStats::default(),
        }
    }

    /// Add a packet to the buffer
    pub fn push(&mut self, seq: u16, packet: T) -> JitterResult<T> {
        self.stats.packets_received += 1;

        // Check for duplicate
        if self.packets.contains_key(&seq) {
            self.stats.packets_duplicate += 1;
            return JitterResult::Duplicate;
        }

        // Check if too late
        let distance = seq.wrapping_sub(self.next_seq);
        if distance > 0x8000 && distance < 0xFFFF - self.late_threshold as u16 {
            self.stats.packets_late += 1;
            return JitterResult::TooLate;
        }

        // Check for overflow
        let overflow_packet = if self.packets.len() >= self.max_size {
            self.stats.packets_overflow += 1;
            // Remove oldest
            self.packets.pop_first().map(|(_, e)| e.packet)
        } else {
            None
        };

        // Insert packet
        self.packets.insert(seq, PacketEntry {
            packet,
            received_at: Instant::now(),
        });

        self.stats.current_depth = self.packets.len();

        match overflow_packet {
            Some(p) => JitterResult::Overflow(p),
            None => JitterResult::Buffered,
        }
    }

    /// Get the next packet in sequence
    pub fn pop(&mut self) -> NextPacket<T> {
        // Check if we have enough buffered
        if self.packets.len() < self.target_depth {
            return NextPacket::Wait;
        }

        // Check if next sequence number is available
        if let Some(entry) = self.packets.remove(&self.next_seq) {
            self.next_seq = self.next_seq.wrapping_add(1);
            self.stats.current_depth = self.packets.len();
            return NextPacket::Ready(entry.packet);
        }

        // Check for gap
        if let Some((&available_seq, _)) = self.packets.first_key_value() {
            return NextPacket::Gap {
                expected: self.next_seq,
                available: available_seq,
            };
        }

        NextPacket::Wait
    }

    /// Skip to a specific sequence number
    pub fn skip_to(&mut self, seq: u16) {
        self.next_seq = seq;
        // Remove any packets before this sequence
        self.packets.retain(|&s, _| {
            let distance = s.wrapping_sub(seq);
            distance < 0x8000
        });
        self.stats.current_depth = self.packets.len();
    }

    /// Get current statistics
    pub fn stats(&self) -> JitterStats {
        JitterStats {
            current_depth: self.packets.len(),
            ..self.stats.clone()
        }
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.packets.clear();
        self.stats.current_depth = 0;
    }

    /// Set the target depth
    pub fn set_target_depth(&mut self, depth: usize) {
        self.target_depth = depth;
    }

    /// Get buffer depth in packets
    pub fn depth(&self) -> usize {
        self.packets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_order_packets() {
        let mut buffer = JitterBuffer::new(2, 10);

        buffer.push(0, "packet0");
        buffer.push(1, "packet1");
        buffer.push(2, "packet2");

        assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
    }

    #[test]
    fn test_out_of_order_packets() {
        let mut buffer = JitterBuffer::new(2, 10);

        // Packets arrive out of order
        buffer.push(1, "packet1");
        buffer.push(0, "packet0");
        buffer.push(2, "packet2");

        // Should still come out in order
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
    }

    #[test]
    fn test_duplicate_detection() {
        let mut buffer = JitterBuffer::new(2, 10);

        buffer.push(0, "packet0");
        let result = buffer.push(0, "duplicate");

        assert!(matches!(result, JitterResult::Duplicate));
    }

    #[test]
    fn test_gap_detection() {
        let mut buffer = JitterBuffer::new(2, 10);

        buffer.push(0, "packet0");
        buffer.push(2, "packet2"); // Skip 1

        buffer.pop(); // Get packet0, next expected is 1

        // Next pop should detect gap
        match buffer.pop() {
            NextPacket::Gap { expected: 1, available: 2 } => {}
            other => panic!("Expected Gap, got {:?}", other),
        }
    }
}
```

---

### 12.3 Audio Clock

- [ ] **12.3.1** Implement audio clock for timing

**File:** `src/audio/clock.rs`

```rust
//! Audio clock for timing synchronization

use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};

/// Audio clock for tracking playback position
pub struct AudioClock {
    /// Sample rate
    sample_rate: u32,
    /// Current frame position
    frame_position: AtomicU64,
    /// Clock offset (for sync)
    offset_micros: AtomicI64,
    /// Reference time for drift calculation
    reference_time: Instant,
    /// Reference frame for drift calculation
    reference_frame: AtomicU64,
}

impl AudioClock {
    /// Create a new audio clock
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            frame_position: AtomicU64::new(0),
            offset_micros: AtomicI64::new(0),
            reference_time: Instant::now(),
            reference_frame: AtomicU64::new(0),
        }
    }

    /// Get current frame position
    pub fn position(&self) -> u64 {
        self.frame_position.load(Ordering::Acquire)
    }

    /// Get current time position
    pub fn time_position(&self) -> Duration {
        let frames = self.position();
        Duration::from_secs_f64(frames as f64 / self.sample_rate as f64)
    }

    /// Advance clock by number of frames
    pub fn advance(&self, frames: u64) {
        self.frame_position.fetch_add(frames, Ordering::Release);
    }

    /// Set position directly
    pub fn set_position(&self, frames: u64) {
        self.frame_position.store(frames, Ordering::Release);
    }

    /// Apply clock offset (for network sync)
    pub fn set_offset(&self, offset_micros: i64) {
        self.offset_micros.store(offset_micros, Ordering::Release);
    }

    /// Get clock offset
    pub fn offset(&self) -> i64 {
        self.offset_micros.load(Ordering::Acquire)
    }

    /// Convert frames to duration
    pub fn frames_to_duration(&self, frames: u64) -> Duration {
        Duration::from_secs_f64(frames as f64 / self.sample_rate as f64)
    }

    /// Convert duration to frames
    pub fn duration_to_frames(&self, duration: Duration) -> u64 {
        (duration.as_secs_f64() * self.sample_rate as f64) as u64
    }

    /// Convert RTP timestamp to local frame position
    pub fn rtp_to_local(&self, rtp_timestamp: u32) -> u64 {
        // RTP timestamps wrap at 32 bits
        // This is a simplified conversion - real implementation needs
        // to track the epoch for proper wrap handling
        rtp_timestamp as u64
    }

    /// Calculate drift from reference
    pub fn calculate_drift(&self) -> f64 {
        let elapsed = self.reference_time.elapsed();
        let expected_frames = (elapsed.as_secs_f64() * self.sample_rate as f64) as u64;
        let actual_frames = self.position() - self.reference_frame.load(Ordering::Acquire);

        if expected_frames == 0 {
            return 0.0;
        }

        (actual_frames as f64 - expected_frames as f64) / expected_frames as f64
    }

    /// Reset the clock
    pub fn reset(&mut self) {
        self.frame_position.store(0, Ordering::Release);
        self.offset_micros.store(0, Ordering::Release);
        self.reference_time = Instant::now();
        self.reference_frame.store(0, Ordering::Release);
    }
}

/// Timing synchronizer for AirPlay
pub struct TimingSync {
    /// Local audio clock
    clock: AudioClock,
    /// Remote clock offset
    remote_offset: AtomicI64,
    /// Round-trip time estimate
    rtt_micros: AtomicU64,
    /// Sync quality (0.0 - 1.0)
    sync_quality: std::sync::atomic::AtomicU32,
}

impl TimingSync {
    /// Create a new timing synchronizer
    pub fn new(sample_rate: u32) -> Self {
        Self {
            clock: AudioClock::new(sample_rate),
            remote_offset: AtomicI64::new(0),
            rtt_micros: AtomicU64::new(0),
            sync_quality: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Update sync with timing response
    pub fn update_timing(&self, offset_micros: i64, rtt_micros: u64) {
        self.remote_offset.store(offset_micros, Ordering::Release);
        self.rtt_micros.store(rtt_micros, Ordering::Release);

        // Calculate sync quality based on RTT stability
        // (simplified - real implementation would use variance)
        let quality = (1.0 - (rtt_micros as f64 / 100_000.0).min(1.0)) as f32;
        self.sync_quality.store(quality.to_bits(), Ordering::Release);
    }

    /// Get the audio clock
    pub fn clock(&self) -> &AudioClock {
        &self.clock
    }

    /// Get current sync offset
    pub fn offset(&self) -> i64 {
        self.remote_offset.load(Ordering::Acquire)
    }

    /// Get current RTT
    pub fn rtt(&self) -> Duration {
        Duration::from_micros(self.rtt_micros.load(Ordering::Acquire))
    }

    /// Get sync quality (0.0 - 1.0)
    pub fn quality(&self) -> f32 {
        f32::from_bits(self.sync_quality.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_advance() {
        let clock = AudioClock::new(44100);

        clock.advance(44100);
        assert_eq!(clock.position(), 44100);

        let duration = clock.time_position();
        assert!((duration.as_secs_f64() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_frame_duration_conversion() {
        let clock = AudioClock::new(48000);

        let frames = clock.duration_to_frames(Duration::from_secs(2));
        assert_eq!(frames, 96000);

        let duration = clock.frames_to_duration(48000);
        assert_eq!(duration.as_secs(), 1);
    }
}
```

---

## Acceptance Criteria

- [ ] Ring buffer handles wrap-around correctly
- [ ] Jitter buffer reorders out-of-order packets
- [ ] Jitter buffer detects gaps and duplicates
- [ ] Audio clock tracks position accurately
- [ ] Timing sync calculates offsets
- [ ] All unit tests pass

---

## Notes

- Ring buffer assumes single-producer/single-consumer
- Jitter buffer depth should be tuned based on network conditions
- Consider adaptive jitter buffer sizing
- Clock drift compensation may need PLL-style approach
