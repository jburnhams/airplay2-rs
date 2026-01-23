//! Audio clock for timing synchronization

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
    #[must_use] 
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
    #[allow(clippy::cast_precision_loss)]
    pub fn time_position(&self) -> Duration {
        let frames = self.position();
        Duration::from_secs_f64(frames as f64 / f64::from(self.sample_rate))
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
    #[allow(clippy::cast_precision_loss)]
    pub fn frames_to_duration(&self, frames: u64) -> Duration {
        Duration::from_secs_f64(frames as f64 / f64::from(self.sample_rate))
    }

    /// Convert duration to frames
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn duration_to_frames(&self, duration: Duration) -> u64 {
        (duration.as_secs_f64() * f64::from(self.sample_rate)) as u64
    }

    /// Convert RTP timestamp to local frame position
    #[allow(clippy::unused_self)]
    pub fn rtp_to_local(&self, rtp_timestamp: u32) -> u64 {
        // RTP timestamps wrap at 32 bits
        // This is a simplified conversion - real implementation needs
        // to track the epoch for proper wrap handling
        u64::from(rtp_timestamp)
    }

    /// Calculate drift from reference
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn calculate_drift(&self) -> f64 {
        let elapsed = self.reference_time.elapsed();
        let expected_frames = (elapsed.as_secs_f64() * f64::from(self.sample_rate)) as u64;
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

/// Timing synchronizer for `AirPlay`
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
    #[must_use] 
    pub fn new(sample_rate: u32) -> Self {
        Self {
            clock: AudioClock::new(sample_rate),
            remote_offset: AtomicI64::new(0),
            rtt_micros: AtomicU64::new(0),
            sync_quality: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Update sync with timing response
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn update_timing(&self, offset_micros: i64, rtt_micros: u64) {
        self.remote_offset.store(offset_micros, Ordering::Release);
        self.rtt_micros.store(rtt_micros, Ordering::Release);

        // Calculate sync quality based on RTT stability
        // (simplified - real implementation would use variance)
        let quality = (1.0 - (rtt_micros as f64 / 100_000.0).min(1.0)) as f32;
        self.sync_quality
            .store(quality.to_bits(), Ordering::Release);
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
