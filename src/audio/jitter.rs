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
    Gap {
        /// Sequence number expected
        expected: u16,
        /// Next available sequence number
        available: u16,
    },
}

impl<T> JitterBuffer<T> {
    /// Create a new jitter buffer
    #[must_use]
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
        if distance > 0x8000 && distance < 0xFFFF - self.late_threshold {
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
        self.packets.insert(
            seq,
            PacketEntry {
                packet,
                received_at: Instant::now(),
            },
        );

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
    #[must_use]
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
    #[must_use]
    pub fn depth(&self) -> usize {
        self.packets.len()
    }
}
