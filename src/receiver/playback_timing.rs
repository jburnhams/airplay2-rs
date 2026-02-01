//! RTP timestamp to playback time mapping

use super::control_receiver::SyncPacket;
use super::timing::{ClockSync, NtpTimestamp};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Maps RTP timestamps to wall-clock time for playback scheduling
pub struct PlaybackTiming {
    /// Sample rate (typically 44100)
    sample_rate: u32,
    /// Reference RTP timestamp (from sync packet)
    ref_rtp_timestamp: Option<u32>,
    /// Reference NTP timestamp (from sync packet)
    #[allow(dead_code)] // Stored for debugging
    ref_ntp_timestamp: Option<NtpTimestamp>,
    /// Reference local time
    ref_local_time: Option<Instant>,
    /// Clock sync for offset
    #[allow(dead_code)] // Used for future drift compensation
    clock_sync: Arc<RwLock<ClockSync>>,
    /// Target latency in samples
    target_latency_samples: u32,
}

impl PlaybackTiming {
    /// Create new playback timing mapper
    #[must_use]
    pub fn new(sample_rate: u32, clock_sync: Arc<RwLock<ClockSync>>) -> Self {
        Self {
            sample_rate,
            ref_rtp_timestamp: None,
            ref_ntp_timestamp: None,
            ref_local_time: None,
            clock_sync,
            // Default 2 second latency
            target_latency_samples: sample_rate * 2,
        }
    }

    /// Update reference from sync packet
    pub fn update_from_sync(&mut self, sync: &SyncPacket) {
        self.ref_rtp_timestamp = Some(sync.rtp_timestamp_at_ntp);
        self.ref_ntp_timestamp = Some(NtpTimestamp::from_u64(sync.ntp_timestamp));
        self.ref_local_time = Some(Instant::now());

        tracing::debug!(
            "Sync update: RTP {} at NTP {}",
            sync.rtp_timestamp_at_ntp,
            sync.ntp_timestamp
        );
    }

    /// Set target latency
    pub fn set_target_latency(&mut self, samples: u32) {
        self.target_latency_samples = samples;
    }

    /// Get target latency in duration
    #[must_use]
    pub fn target_latency(&self) -> Duration {
        Duration::from_secs_f64(f64::from(self.target_latency_samples) / f64::from(self.sample_rate))
    }

    /// Calculate when an RTP timestamp should be played
    ///
    /// Returns the Instant at which the audio with this RTP timestamp
    /// should be sent to the audio device.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn playback_time(&self, rtp_timestamp: u32) -> Option<Instant> {
        let ref_rtp = self.ref_rtp_timestamp?;
        let ref_local = self.ref_local_time?;

        // Calculate samples since reference
        let samples_diff = i64::from(rtp_timestamp.wrapping_sub(ref_rtp));

        // Convert to duration
        let time_diff =
            Duration::from_secs_f64(samples_diff as f64 / f64::from(self.sample_rate));

        // Add target latency
        let latency = self.target_latency();

        // Calculate playback time
        let playback_time = if samples_diff >= 0 {
            ref_local + time_diff + latency
        } else {
            // Past timestamp
            ref_local.checked_sub(Duration::from_secs_f64(
                (-samples_diff) as f64 / f64::from(self.sample_rate),
            ))? + latency
        };

        Some(playback_time)
    }

    /// Check if an RTP timestamp is ready for playback
    #[must_use]
    pub fn is_ready_for_playback(&self, rtp_timestamp: u32) -> bool {
        if let Some(playback_time) = self.playback_time(rtp_timestamp) {
            Instant::now() >= playback_time
        } else {
            // No sync yet, use simple delay
            false
        }
    }

    /// Get delay until playback time
    #[must_use]
    pub fn delay_until_playback(&self, rtp_timestamp: u32) -> Option<Duration> {
        let playback_time = self.playback_time(rtp_timestamp)?;
        let now = Instant::now();

        if now >= playback_time {
            Some(Duration::ZERO)
        } else {
            Some(playback_time - now)
        }
    }

    /// Convert RTP timestamp difference to duration
    #[must_use]
    pub fn rtp_to_duration(&self, rtp_samples: u32) -> Duration {
        Duration::from_secs_f64(f64::from(rtp_samples) / f64::from(self.sample_rate))
    }
}
