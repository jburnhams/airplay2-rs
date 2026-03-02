//! RTP sequence number tracking and packet loss detection

use std::collections::VecDeque;

/// Tracks RTP sequence numbers to detect gaps
pub struct SequenceTracker {
    /// Last received sequence number
    last_seq: Option<u16>,
    /// Expected next sequence number
    expected_seq: Option<u16>,
    /// Recent gap history for statistics
    recent_gaps: VecDeque<GapInfo>,
    /// Maximum history size
    max_history: usize,
    /// Total packets received
    packets_received: u64,
    /// Total gaps detected
    total_gaps: u64,
    /// Total packets lost
    total_lost: u64,
}

/// Information about a detected gap
#[derive(Debug, Clone)]
pub struct GapInfo {
    /// First missing sequence
    pub start: u16,
    /// Count of missing packets
    pub count: u16,
    /// When gap was detected
    pub detected_at: std::time::Instant,
}

impl SequenceTracker {
    /// Create a new sequence tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_seq: None,
            expected_seq: None,
            recent_gaps: VecDeque::with_capacity(100),
            max_history: 100,
            packets_received: 0,
            total_gaps: 0,
            total_lost: 0,
        }
    }

    /// Record a received packet, returning any detected gap
    pub fn record(&mut self, seq: u16) -> Option<GapInfo> {
        self.packets_received += 1;

        let gap = if let Some(expected) = self.expected_seq {
            let gap_size = Self::sequence_gap(expected, seq);

            if gap_size > 0 && gap_size < 1000 {
                // Gap detected (but not wrap-around)
                self.total_gaps += 1;
                self.total_lost += u64::from(gap_size);

                let gap_info = GapInfo {
                    start: expected,
                    count: gap_size,
                    detected_at: std::time::Instant::now(),
                };

                if self.recent_gaps.len() >= self.max_history {
                    self.recent_gaps.pop_front();
                }
                self.recent_gaps.push_back(gap_info.clone());

                Some(gap_info)
            } else {
                None
            }
        } else {
            None
        };

        self.last_seq = Some(seq);
        self.expected_seq = Some(seq.wrapping_add(1));

        gap
    }

    /// Calculate gap between expected and actual sequence numbers
    /// Handles 16-bit wraparound correctly
    fn sequence_gap(expected: u16, actual: u16) -> u16 {
        actual.wrapping_sub(expected)
    }

    /// Check if a sequence number is expected (not duplicate, not too old)
    #[must_use]
    pub fn is_expected(&self, seq: u16) -> bool {
        if let Some(expected) = self.expected_seq {
            let diff = seq.wrapping_sub(expected);
            // Accept if within reasonable window (ahead or slightly behind)
            // diff < 1000 || diff > 65000
            !(1000..=65000).contains(&diff)
        } else {
            true // First packet
        }
    }

    /// Get packet loss ratio (0.0 to 1.0)
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "Precision loss acceptable for ratio calculation"
    )]
    pub fn loss_ratio(&self) -> f64 {
        if self.packets_received == 0 {
            return 0.0;
        }
        let total = self.packets_received + self.total_lost;
        self.total_lost as f64 / total as f64
    }

    /// Get statistics
    #[must_use]
    pub fn stats(&self) -> SequenceStats {
        SequenceStats {
            packets_received: self.packets_received,
            total_gaps: self.total_gaps,
            total_lost: self.total_lost,
            loss_ratio: self.loss_ratio(),
        }
    }

    /// Reset the tracker
    pub fn reset(&mut self) {
        self.last_seq = None;
        self.expected_seq = None;
        self.recent_gaps.clear();
        self.packets_received = 0;
        self.total_gaps = 0;
        self.total_lost = 0;
    }
}

/// Statistics for sequence tracking
#[derive(Debug, Clone)]
pub struct SequenceStats {
    /// Total packets received
    pub packets_received: u64,
    /// Total gaps detected
    pub total_gaps: u64,
    /// Total packets lost
    pub total_lost: u64,
    /// Loss ratio (0.0 to 1.0)
    pub loss_ratio: f64,
}

impl Default for SequenceTracker {
    fn default() -> Self {
        Self::new()
    }
}
