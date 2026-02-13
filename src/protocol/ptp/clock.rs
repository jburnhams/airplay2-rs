//! PTP clock synchronization.
//!
//! Implements clock offset and drift estimation using the IEEE 1588
//! delay request-response mechanism. Works for both client (master)
//! and receiver (slave) roles.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::timestamp::PtpTimestamp;

/// Role of this PTP participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpRole {
    /// Master clock (sender/client) — originates Sync messages.
    Master,
    /// Slave clock (receiver) — synchronizes to master.
    Slave,
}

/// A single timing measurement from a delay request-response exchange.
#[derive(Debug, Clone)]
pub struct TimingMeasurement {
    /// T1: master send time (origin timestamp from Sync/Follow-up).
    pub t1: PtpTimestamp,
    /// T2: slave receive time (when slave received Sync).
    pub t2: PtpTimestamp,
    /// T3: slave send time (origin timestamp of `Delay_Req`).
    pub t3: PtpTimestamp,
    /// T4: master receive time (from `Delay_Resp`).
    pub t4: PtpTimestamp,
    /// Local wall-clock time when this measurement was recorded.
    pub local_time: Instant,
    /// Calculated offset in nanoseconds: (slave - master).
    /// Positive means slave clock is ahead of master.
    pub offset_ns: i128,
    /// Round-trip time.
    pub rtt: Duration,
}

impl TimingMeasurement {
    /// Calculate offset from timestamps.
    ///
    /// offset = ((T2 - T1) + (T3 - T4)) / 2
    /// This gives (slave - master). Positive means slave is ahead of master.
    #[must_use]
    pub fn calculate(
        t1: PtpTimestamp,
        t2: PtpTimestamp,
        t3: PtpTimestamp,
        t4: PtpTimestamp,
        local_time: Instant,
    ) -> Self {
        let t2_minus_t1 = t2.diff_nanos(&t1);
        let t3_minus_t4 = t3.diff_nanos(&t4);
        // offset = ((t2 - t1) + (t3 - t4)) / 2
        // This is (slave_clock - master_clock), which gives the amount
        // slave is ahead of master.
        let offset_ns = (t2_minus_t1 + t3_minus_t4) / 2;

        // RTT = (t4 - t1) - (t3 - t2) = total round trip minus processing
        let rtt_nanos = (t4.diff_nanos(&t1) - t3.diff_nanos(&t2)).max(0);
        let rtt = Duration::from_nanos(u64::try_from(rtt_nanos).unwrap_or(u64::MAX));

        Self {
            t1,
            t2,
            t3,
            t4,
            local_time,
            offset_ns,
            rtt,
        }
    }
}

/// PTP clock synchronizer.
///
/// Maintains offset and drift estimates between local and remote clocks.
/// Usable from either the master or slave side.
pub struct PtpClock {
    /// Clock identity.
    clock_id: u64,
    /// Role of this clock.
    role: PtpRole,
    /// Recent measurements for filtering.
    measurements: VecDeque<TimingMeasurement>,
    /// Maximum number of measurements to keep.
    max_measurements: usize,
    /// Current offset estimate (slave - master) in nanoseconds.
    /// Positive means slave clock is ahead of master.
    offset_ns: i128,
    /// Current drift rate in parts-per-million.
    drift_ppm: f64,
    /// Whether we have enough data to be considered synchronized.
    synchronized: bool,
    /// Minimum measurements needed before marking as synchronized.
    min_sync_measurements: usize,
    /// Maximum RTT to accept a measurement (outlier rejection).
    max_rtt: Duration,
}

impl PtpClock {
    /// Default maximum measurements to retain.
    pub const DEFAULT_MAX_MEASUREMENTS: usize = 8;

    /// Default minimum measurements before synchronization.
    pub const DEFAULT_MIN_SYNC: usize = 1;

    /// Default maximum RTT for accepting a measurement.
    pub const DEFAULT_MAX_RTT: Duration = Duration::from_millis(100);

    /// Create a new PTP clock.
    #[must_use]
    pub fn new(clock_id: u64, role: PtpRole) -> Self {
        Self {
            clock_id,
            role,
            measurements: VecDeque::new(),
            max_measurements: Self::DEFAULT_MAX_MEASUREMENTS,
            offset_ns: 0,
            drift_ppm: 0.0,
            synchronized: false,
            min_sync_measurements: Self::DEFAULT_MIN_SYNC,
            max_rtt: Self::DEFAULT_MAX_RTT,
        }
    }

    /// Set the maximum number of measurements to retain.
    pub fn set_max_measurements(&mut self, max: usize) {
        self.max_measurements = max.max(1);
    }

    /// Set the minimum measurements required for synchronization.
    pub fn set_min_sync_measurements(&mut self, min: usize) {
        self.min_sync_measurements = min.max(1);
    }

    /// Set the maximum RTT for accepting measurements.
    pub fn set_max_rtt(&mut self, max_rtt: Duration) {
        self.max_rtt = max_rtt;
    }

    /// Process a complete timing exchange (four timestamps).
    ///
    /// - T1: master send time (from Sync / Follow-up)
    /// - T2: slave receive time
    /// - T3: slave send time (`Delay_Req` origin)
    /// - T4: master receive time (from `Delay_Resp`)
    ///
    /// Returns `true` if the measurement was accepted.
    pub fn process_timing(
        &mut self,
        t1: PtpTimestamp,
        t2: PtpTimestamp,
        t3: PtpTimestamp,
        t4: PtpTimestamp,
    ) -> bool {
        let measurement = TimingMeasurement::calculate(t1, t2, t3, t4, Instant::now());

        // Reject outliers based on RTT.
        if measurement.rtt > self.max_rtt {
            tracing::debug!(
                rtt = ?measurement.rtt,
                max_rtt = ?self.max_rtt,
                "PTP: rejecting measurement with excessive RTT"
            );
            return false;
        }

        self.measurements.push_back(measurement);
        while self.measurements.len() > self.max_measurements {
            self.measurements.pop_front();
        }

        self.update_offset();
        self.update_drift();

        if self.measurements.len() >= self.min_sync_measurements {
            self.synchronized = true;
        }

        true
    }

    /// Update offset using the median of recent measurements.
    fn update_offset(&mut self) {
        if self.measurements.is_empty() {
            return;
        }
        let mut offsets: Vec<i128> = self.measurements.iter().map(|m| m.offset_ns).collect();
        offsets.sort_unstable();
        self.offset_ns = offsets[offsets.len() / 2];
    }

    /// Update drift rate using linear regression on offset vs. time.
    fn update_drift(&mut self) {
        if self.measurements.len() < 2 {
            return;
        }

        let first = self.measurements.front().unwrap();
        let last = self.measurements.back().unwrap();

        let time_diff_secs = last
            .local_time
            .duration_since(first.local_time)
            .as_secs_f64();
        if time_diff_secs < 0.1 {
            return; // Need more elapsed time for meaningful drift estimate.
        }

        // Simple two-point drift estimate. For production, a full linear regression
        // over all measurements would be more robust.
        #[allow(clippy::cast_precision_loss)]
        let offset_diff_ns = (last.offset_ns - first.offset_ns) as f64;
        // Drift in ppm: (offset change in ns) / (time in ns) * 1e6
        self.drift_ppm = offset_diff_ns / (time_diff_secs * 1e9) * 1e6;
    }

    /// Convert a remote PTP timestamp to an equivalent local PTP timestamp.
    ///
    /// Adjusts for offset and drift.
    #[must_use]
    pub fn remote_to_local(&self, remote: PtpTimestamp) -> PtpTimestamp {
        // local = remote - offset (since offset = slave - master)
        let remote_nanos = remote.to_nanos();
        let local_nanos = remote_nanos - self.offset_ns;
        if local_nanos < 0 {
            return PtpTimestamp::ZERO;
        }
        PtpTimestamp::from_nanos(local_nanos)
    }

    /// Convert a local PTP timestamp to an equivalent remote PTP timestamp.
    ///
    /// Adjusts for offset and drift.
    #[must_use]
    pub fn local_to_remote(&self, local: PtpTimestamp) -> PtpTimestamp {
        // remote = local + offset
        let local_nanos = local.to_nanos();
        let remote_nanos = local_nanos + self.offset_ns;
        if remote_nanos < 0 {
            return PtpTimestamp::ZERO;
        }
        PtpTimestamp::from_nanos(remote_nanos)
    }

    /// Get the current offset estimate in nanoseconds.
    ///
    /// Positive means slave clock is ahead of master.
    #[must_use]
    pub fn offset_nanos(&self) -> i128 {
        self.offset_ns
    }

    /// Get the current offset estimate in microseconds.
    #[must_use]
    pub fn offset_micros(&self) -> i64 {
        i64::try_from(self.offset_ns / 1_000).unwrap_or(i64::MAX)
    }

    /// Get the current offset estimate in milliseconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn offset_millis(&self) -> f64 {
        self.offset_ns as f64 / 1_000_000.0
    }

    /// Get the drift rate in parts-per-million.
    #[must_use]
    pub fn drift_ppm(&self) -> f64 {
        self.drift_ppm
    }

    /// Whether the clock is considered synchronized.
    #[must_use]
    pub fn is_synchronized(&self) -> bool {
        self.synchronized
    }

    /// Get the clock identity.
    #[must_use]
    pub fn clock_id(&self) -> u64 {
        self.clock_id
    }

    /// Get the role.
    #[must_use]
    pub fn role(&self) -> PtpRole {
        self.role
    }

    /// Get the number of measurements currently held.
    #[must_use]
    pub fn measurement_count(&self) -> usize {
        self.measurements.len()
    }

    /// Get the most recent RTT, if any measurement exists.
    #[must_use]
    pub fn last_rtt(&self) -> Option<Duration> {
        self.measurements.back().map(|m| m.rtt)
    }

    /// Get the median RTT across stored measurements.
    #[must_use]
    pub fn median_rtt(&self) -> Option<Duration> {
        if self.measurements.is_empty() {
            return None;
        }
        let mut rtts: Vec<Duration> = self.measurements.iter().map(|m| m.rtt).collect();
        rtts.sort_unstable();
        Some(rtts[rtts.len() / 2])
    }

    /// Reset the clock, clearing all measurements.
    pub fn reset(&mut self) {
        self.measurements.clear();
        self.offset_ns = 0;
        self.drift_ppm = 0.0;
        self.synchronized = false;
    }

    /// Get all stored measurements (for diagnostics).
    pub fn measurements(&self) -> impl Iterator<Item = &TimingMeasurement> {
        self.measurements.iter()
    }

    /// Convert an RTP timestamp to a local PTP timestamp.
    ///
    /// Uses the sample rate to convert from samples to time.
    #[must_use]
    pub fn rtp_to_local_ptp(
        &self,
        rtp_timestamp: u32,
        sample_rate: u32,
        rtp_anchor: u32,
        ptp_anchor: PtpTimestamp,
    ) -> PtpTimestamp {
        let sample_diff = i64::from(rtp_timestamp.wrapping_sub(rtp_anchor));
        let nanos_diff = sample_diff * 1_000_000_000 / i64::from(sample_rate);
        let remote_ptp_nanos = ptp_anchor.to_nanos() + i128::from(nanos_diff);
        let remote_ptp = if remote_ptp_nanos >= 0 {
            PtpTimestamp::from_nanos(remote_ptp_nanos)
        } else {
            PtpTimestamp::ZERO
        };
        self.remote_to_local(remote_ptp)
    }
}

impl std::fmt::Debug for PtpClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtpClock")
            .field("clock_id", &format_args!("0x{:016X}", self.clock_id))
            .field("role", &self.role)
            .field("synchronized", &self.synchronized)
            .field("offset_ms", &self.offset_millis())
            .field("drift_ppm", &self.drift_ppm)
            .field("measurements", &self.measurements.len())
            .finish_non_exhaustive()
    }
}
