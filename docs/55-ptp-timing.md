# Section 55: PTP Timing Synchronization

## Dependencies
- **Section 52**: Multi-Phase SETUP Handler (timing channel)
- **Section 40**: Timing Synchronization (AirPlay 1 patterns)

## Overview

AirPlay 2 uses Precision Time Protocol (PTP, IEEE 1588) for timing synchronization, enabling accurate multi-room playback. The receiver must synchronize its clock with the sender for sample-accurate audio output.

### PTP vs NTP-style Timing

| Aspect | NTP-style (AirPlay 1) | PTP (AirPlay 2) |
|--------|----------------------|-----------------|
| Precision | ~1-10ms | <1ms |
| Protocol | Custom UDP | IEEE 1588 |
| Multi-room | Limited | Full support |
| Clock model | Offset only | Offset + rate |

## Objectives

- Implement PTP message parsing and generation
- Calculate clock offset from timing exchanges
- Estimate clock drift rate
- Provide timestamp conversion for audio output
- Support timing channel on allocated UDP port

---

## Tasks

### 55.1 PTP Clock Implementation

- [ ] **55.1.1** Implement PTP clock synchronization

**File:** `src/receiver/ap2/ptp_clock.rs`

```rust
//! PTP Timing Synchronization for AirPlay 2
//!
//! Implements IEEE 1588 Precision Time Protocol for clock synchronization
//! between sender and receiver.

use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// PTP clock synchronizer
pub struct PtpClock {
    /// Clock ID (from advertisement)
    clock_id: u64,
    /// Reference time (local)
    reference_local: Instant,
    /// Reference time (remote, in PTP timestamp units)
    reference_remote: u64,
    /// Estimated offset (remote - local) in nanoseconds
    offset_ns: i64,
    /// Estimated drift rate (ppm)
    drift_ppm: f64,
    /// Recent measurements for filtering
    measurements: VecDeque<TimingMeasurement>,
    /// Max measurements to keep
    max_measurements: usize,
    /// Synchronized flag
    synchronized: bool,
}

/// Single timing measurement
#[derive(Debug, Clone)]
struct TimingMeasurement {
    /// Local send time
    t1: Instant,
    /// Remote receive time (from response)
    t2: u64,
    /// Remote send time (from response)
    t3: u64,
    /// Local receive time
    t4: Instant,
    /// Calculated offset
    offset_ns: i64,
    /// Round-trip time
    rtt: Duration,
}

/// PTP timing message types
#[derive(Debug, Clone, Copy)]
pub enum PtpMessageType {
    Sync = 0x00,
    DelayReq = 0x01,
    FollowUp = 0x08,
    DelayResp = 0x09,
    Announce = 0x0B,
}

/// PTP timing request/response
#[derive(Debug, Clone)]
pub struct PtpTimingPacket {
    pub message_type: PtpMessageType,
    pub sequence_id: u16,
    pub timestamp: u64,  // 48.16 fixed point (seconds.fraction)
    pub clock_id: u64,
}

impl PtpClock {
    /// Create a new PTP clock
    pub fn new(clock_id: u64) -> Self {
        Self {
            clock_id,
            reference_local: Instant::now(),
            reference_remote: 0,
            offset_ns: 0,
            drift_ppm: 0.0,
            measurements: VecDeque::new(),
            max_measurements: 8,
            synchronized: false,
        }
    }

    /// Process a timing exchange
    pub fn process_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        // Calculate round-trip time
        let rtt = t4.duration_since(t1);

        // Calculate offset: ((t2 - t1) + (t3 - t4)) / 2
        // In nanoseconds
        let t1_ns = 0i64;  // Reference point
        let t4_ns = rtt.as_nanos() as i64;
        let t2_ns = Self::ptp_to_nanos(t2);
        let t3_ns = Self::ptp_to_nanos(t3);

        let offset_ns = ((t2_ns - t1_ns) + (t3_ns - t4_ns)) / 2;

        let measurement = TimingMeasurement {
            t1,
            t2,
            t3,
            t4,
            offset_ns,
            rtt,
        };

        // Add to history
        self.measurements.push_back(measurement);
        if self.measurements.len() > self.max_measurements {
            self.measurements.pop_front();
        }

        // Update offset estimate (filtered)
        self.update_offset();

        // Update drift estimate
        self.update_drift();

        self.synchronized = true;
    }

    /// Update offset using median filter
    fn update_offset(&mut self) {
        if self.measurements.is_empty() {
            return;
        }

        let mut offsets: Vec<i64> = self.measurements.iter()
            .map(|m| m.offset_ns)
            .collect();
        offsets.sort();

        // Use median for robustness
        self.offset_ns = offsets[offsets.len() / 2];

        // Update reference
        if let Some(last) = self.measurements.back() {
            self.reference_local = last.t4;
            self.reference_remote = last.t3;
        }
    }

    /// Update drift rate estimate
    fn update_drift(&mut self) {
        if self.measurements.len() < 2 {
            return;
        }

        // Linear regression on offset vs time
        let first = self.measurements.front().unwrap();
        let last = self.measurements.back().unwrap();

        let time_diff = last.t4.duration_since(first.t4).as_secs_f64();
        if time_diff < 1.0 {
            return;  // Need more data
        }

        let offset_diff = (last.offset_ns - first.offset_ns) as f64;

        // Drift in ppm
        self.drift_ppm = (offset_diff / time_diff) / 1000.0;
    }

    /// Convert local timestamp to remote PTP timestamp
    pub fn local_to_remote(&self, local: Instant) -> u64 {
        let elapsed = local.duration_since(self.reference_local);
        let elapsed_ns = elapsed.as_nanos() as i64;

        // Apply offset and drift correction
        let drift_correction = (elapsed_ns as f64 * self.drift_ppm / 1_000_000.0) as i64;
        let remote_ns = elapsed_ns + self.offset_ns + drift_correction;

        Self::nanos_to_ptp(Self::ptp_to_nanos(self.reference_remote) + remote_ns)
    }

    /// Convert remote PTP timestamp to local timestamp
    pub fn remote_to_local(&self, remote: u64) -> Instant {
        let remote_ns = Self::ptp_to_nanos(remote);
        let ref_remote_ns = Self::ptp_to_nanos(self.reference_remote);

        let diff_ns = remote_ns - ref_remote_ns;

        // Remove offset and drift
        let drift_correction = (diff_ns as f64 * self.drift_ppm / 1_000_000.0) as i64;
        let local_diff_ns = diff_ns - self.offset_ns - drift_correction;

        self.reference_local + Duration::from_nanos(local_diff_ns.max(0) as u64)
    }

    /// Convert RTP timestamp to local playback time
    pub fn rtp_to_playback_time(&self, rtp_timestamp: u32, sample_rate: u32) -> Instant {
        // RTP timestamp is in samples
        let samples_ns = (rtp_timestamp as u64 * 1_000_000_000) / sample_rate as u64;

        // This needs anchor point from sender
        // For now, approximate
        self.reference_local + Duration::from_nanos(samples_ns)
    }

    /// Check if clock is synchronized
    pub fn is_synchronized(&self) -> bool {
        self.synchronized
    }

    /// Get current offset estimate in milliseconds
    pub fn offset_ms(&self) -> f64 {
        self.offset_ns as f64 / 1_000_000.0
    }

    /// Get current drift estimate in ppm
    pub fn drift_ppm(&self) -> f64 {
        self.drift_ppm
    }

    /// Get clock ID
    pub fn clock_id(&self) -> u64 {
        self.clock_id
    }

    // Convert PTP 48.16 fixed point to nanoseconds
    fn ptp_to_nanos(ptp: u64) -> i64 {
        let seconds = (ptp >> 16) as i64;
        let fraction = (ptp & 0xFFFF) as i64;
        seconds * 1_000_000_000 + (fraction * 1_000_000_000 / 65536)
    }

    // Convert nanoseconds to PTP 48.16 fixed point
    fn nanos_to_ptp(nanos: i64) -> u64 {
        let seconds = nanos / 1_000_000_000;
        let remainder = nanos % 1_000_000_000;
        let fraction = (remainder * 65536 / 1_000_000_000) as u64;
        ((seconds as u64) << 16) | fraction
    }
}

impl PtpTimingPacket {
    /// Parse a PTP timing packet
    pub fn parse(data: &[u8]) -> Result<Self, TimingParseError> {
        if data.len() < 16 {
            return Err(TimingParseError::TooShort);
        }

        let message_type = match data[0] & 0x0F {
            0x00 => PtpMessageType::Sync,
            0x01 => PtpMessageType::DelayReq,
            0x08 => PtpMessageType::FollowUp,
            0x09 => PtpMessageType::DelayResp,
            0x0B => PtpMessageType::Announce,
            t => return Err(TimingParseError::UnknownType(t)),
        };

        let sequence_id = u16::from_be_bytes([data[2], data[3]]);

        // Timestamp at offset 8 (48.16 fixed point)
        let timestamp = u64::from_be_bytes([
            0, 0, data[8], data[9], data[10], data[11], data[12], data[13]
        ]);

        let clock_id = u64::from_be_bytes([
            data[14], data[15], data[16], data[17],
            data[18], data[19], data[20], data[21]
        ]);

        Ok(Self {
            message_type,
            sequence_id,
            timestamp,
            clock_id,
        })
    }

    /// Encode a timing packet
    pub fn encode(&self) -> Vec<u8> {
        let mut data = vec![0u8; 24];

        data[0] = self.message_type as u8;
        data[2..4].copy_from_slice(&self.sequence_id.to_be_bytes());

        let ts_bytes = self.timestamp.to_be_bytes();
        data[8..14].copy_from_slice(&ts_bytes[2..8]);

        data[14..22].copy_from_slice(&self.clock_id.to_be_bytes());

        data
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TimingParseError {
    #[error("Packet too short")]
    TooShort,
    #[error("Unknown message type: {0}")]
    UnknownType(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptp_conversion() {
        // 1 second = 0x00010000 in PTP format
        let one_second = 0x00010000u64;
        let nanos = PtpClock::ptp_to_nanos(one_second);
        assert_eq!(nanos, 1_000_000_000);

        let back = PtpClock::nanos_to_ptp(nanos);
        assert_eq!(back, one_second);
    }

    #[test]
    fn test_clock_sync() {
        let mut clock = PtpClock::new(0x123456);

        assert!(!clock.is_synchronized());

        // Simulate timing exchange
        let t1 = Instant::now();
        let t2 = 0x00010000u64;  // 1 second
        let t3 = 0x00010001u64;  // 1 second + small delta
        let t4 = t1 + Duration::from_millis(10);

        clock.process_timing(t1, t2, t3, t4);

        assert!(clock.is_synchronized());
    }
}
```

---

## Acceptance Criteria

- [ ] PTP messages parsed correctly
- [ ] Clock offset calculated from timing exchanges
- [ ] Drift rate estimated over time
- [ ] Timestamp conversion between local and remote
- [ ] Median filter for robustness
- [ ] All unit tests pass

---

## References

- [IEEE 1588 PTP](https://www.nist.gov/el/intelligent-systems-division-73500/ieee-1588)
- [AirPlay 2 Timing Analysis](https://emanuelecozzi.net/docs/airplay2/timing/)
