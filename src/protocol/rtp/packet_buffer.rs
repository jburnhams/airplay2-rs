//! Packet buffer for retransmission support

use bytes::Bytes;
use std::collections::VecDeque;

/// Audio packet with sequence tracking
#[derive(Debug, Clone)]
pub struct BufferedPacket {
    /// Sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// Encoded packet data (ready for retransmission)
    pub data: Bytes,
}

/// Circular buffer for recently sent packets
pub struct PacketBuffer {
    /// Maximum buffer size
    max_size: usize,
    /// Buffered packets
    packets: VecDeque<BufferedPacket>,
}

impl PacketBuffer {
    /// Default buffer size (1 second at ~125 packets/sec)
    pub const DEFAULT_SIZE: usize = 128;

    /// Create new packet buffer
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            packets: VecDeque::with_capacity(max_size),
        }
    }

    /// Add a packet to the buffer
    pub fn push(&mut self, packet: BufferedPacket) {
        if self.packets.len() >= self.max_size {
            self.packets.pop_front();
        }
        self.packets.push_back(packet);
    }

    /// Get a packet by sequence number
    #[must_use]
    pub fn get(&self, sequence: u16) -> Option<&BufferedPacket> {
        self.packets.iter().find(|p| p.sequence == sequence)
    }

    /// Get a range of packets for retransmission
    pub fn get_range(&self, start: u16, count: u16) -> impl Iterator<Item = &BufferedPacket> + '_ {
        let mut requested_seqs = (0..count).map(move |i| start.wrapping_add(i)).peekable();

        self.packets.iter().filter(move |packet| {
            while let Some(&seq) = requested_seqs.peek() {
                let diff = packet.sequence.wrapping_sub(seq);
                if diff > 0 && diff < 0x8000 {
                    requested_seqs.next();
                } else {
                    break;
                }
            }

            if let Some(&seq) = requested_seqs.peek() {
                if packet.sequence == seq {
                    requested_seqs.next();
                    return true;
                }
            }

            false
        })
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.packets.clear();
    }

    /// Number of packets in buffer
    #[must_use]
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// Check if buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    /// Get sequence number range
    #[must_use]
    pub fn sequence_range(&self) -> Option<(u16, u16)> {
        if self.packets.is_empty() {
            None
        } else {
            Some((
                self.packets.front()?.sequence,
                self.packets.back()?.sequence,
            ))
        }
    }
}

/// Packet loss detector
#[derive(Default)]
pub struct PacketLossDetector {
    /// Expected next sequence number
    expected_seq: u16,
    /// First sequence received
    first_received: bool,
}

impl PacketLossDetector {
    /// Create new loss detector
    #[must_use]
    pub fn new() -> Self {
        Self {
            expected_seq: 0,
            first_received: false,
        }
    }

    /// Process received sequence number
    ///
    /// Returns list of missing sequence numbers
    pub fn process(&mut self, sequence: u16) -> Vec<u16> {
        if !self.first_received {
            self.first_received = true;
            self.expected_seq = sequence.wrapping_add(1);
            return Vec::new();
        }

        // Calculate how many packets were skipped
        let diff = sequence.wrapping_sub(self.expected_seq);

        // Check for reordered (old) packet
        // If diff is greater than half the range (32768), it means sequence is behind expected_seq
        if diff >= 0x8000 {
            return Vec::new();
        }

        let missing = if diff > 0 && diff < 100 {
            let mut missing = Vec::with_capacity(diff as usize);
            // Packets were lost
            for i in 0..diff {
                missing.push(self.expected_seq.wrapping_add(i));
            }
            missing
        } else {
            Vec::new()
        };

        // Update expected
        self.expected_seq = sequence.wrapping_add(1);

        missing
    }

    /// Reset detector
    pub fn reset(&mut self) {
        self.first_received = false;
        self.expected_seq = 0;
    }
}
