//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use std::time::Instant;

use crate::protocol::ptp::clock::{PtpClock, PtpRole};
use crate::protocol::ptp::timestamp::PtpTimestamp;

/// Group role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// Not in a group
    None,
    /// Group leader (reference clock)
    Leader,
    /// Group follower (syncs to leader)
    Follower,
}

/// Multi-room group information
#[derive(Debug, Clone)]
pub struct GroupInfo {
    /// Group UUID
    pub uuid: String,
    /// Our role in the group
    pub role: GroupRole,
    /// Leader's clock ID (if follower)
    pub leader_clock_id: Option<u64>,
    /// Group members
    pub members: Vec<GroupMember>,
    /// Target playback time (shared across group)
    pub target_playback_time: Option<u64>,
}

/// Group member info
#[derive(Debug, Clone)]
pub struct GroupMember {
    /// Member device ID
    pub device_id: String,
    /// Member display name
    pub name: String,
    /// Member PTP clock ID
    pub clock_id: u64,
    /// Member role in the group
    pub role: GroupRole,
}

/// Multi-room coordinator
pub struct MultiRoomCoordinator {
    /// Our device ID
    #[allow(dead_code)]
    device_id: String,
    /// Our clock
    clock: PtpClock,
    /// Current group info
    group: Option<GroupInfo>,
    /// Sync tolerance (microseconds)
    sync_tolerance_us: i64,
    /// Last sync check
    #[allow(dead_code)]
    last_sync_check: Instant,
    /// Sync status
    in_sync: bool,
}

/// Playback timing command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// PTP timestamp to start playback
        timestamp: u64,
    },
    /// Adjust playback rate to catch up/slow down
    AdjustRate {
        /// Rate adjustment in parts per million
        rate_ppm: i32,
    },
    /// Pause playback
    Pause,
    /// Resume playback
    Resume,
}

impl MultiRoomCoordinator {
    /// Create a new multi-room coordinator
    #[must_use]
    pub fn new(device_id: String, clock_id: u64) -> Self {
        Self {
            device_id,
            // Default to Slave role for PTP clock as we usually sync to a master
            clock: PtpClock::new(clock_id, PtpRole::Slave),
            group: None,
            sync_tolerance_us: 1000, // 1ms default
            last_sync_check: Instant::now(),
            in_sync: false,
        }
    }

    /// Join a group
    pub fn join_group(&mut self, uuid: String, role: GroupRole, leader_clock_id: Option<u64>) {
        self.group = Some(GroupInfo {
            uuid,
            role,
            leader_clock_id,
            members: Vec::new(),
            target_playback_time: None,
        });

        tracing::info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            tracing::info!("Left group");
            self.group = None;
            self.clock.reset();
        }
    }

    /// Set target playback time (from sender)
    pub fn set_target_time(&mut self, timestamp: u64) {
        if let Some(ref mut group) = self.group {
            group.target_playback_time = Some(timestamp);
        }
    }

    /// Calculate playback adjustment needed
    pub fn calculate_adjustment(&mut self) -> Option<PlaybackCommand> {
        let group = self.group.as_ref()?;
        let target = group.target_playback_time?;

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time
        let now = Instant::now();
        let now_ptp = Self::instant_to_ptp(now);
        // Convert local (Slave) time to remote (Master) time to compare with target
        let current_ptp = self.clock.local_to_remote(now_ptp);

        // Target is in remote (Master) time (AirPlay compact format)
        let target_ptp = PtpTimestamp::from_airplay_compact(target);

        // Calculate drift: current - target
        // If drift is positive, we are ahead (need to slow down or wait).
        // If drift is negative, we are behind (need to speed up or skip).
        let drift_micros = current_ptp.diff_micros(&target_ptp);

        self.in_sync = drift_micros.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_micros.abs() > 10_000 {
            // More than 10ms off - hard sync
            tracing::warn!(
                "Multi-room: large drift {}us, requesting hard sync",
                drift_micros
            );
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            // If drift is positive (we are ahead), we want to slow down (negative rate).
            // If drift is negative (we are behind), we want to speed up (positive rate).
            // So rate should be proportional to -drift.
            // rate_ppm = -(drift_us / 10) clamped to +/- 500
            #[allow(clippy::cast_possible_truncation, reason = "Clamped value fits in i32")]
            let rate_ppm = (-drift_micros / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// Arguments:
    /// - `t1`: Local receive time of Sync (T2)
    /// - `t2`: Remote send time of Sync (T1) - `AirPlay` compact
    /// - `t3`: Remote receive time of `Delay_Req` (T4) - `AirPlay` compact
    /// - `t4`: Local send time of `Delay_Req` (T3)
    pub fn update_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        let ptp_t2_local_rx = Self::instant_to_ptp(t1);
        let ptp_t1_remote_tx = PtpTimestamp::from_airplay_compact(t2);
        let ptp_t4_remote_rx = PtpTimestamp::from_airplay_compact(t3);
        let ptp_t3_local_tx = Self::instant_to_ptp(t4);

        // We pass arguments to process_timing such that calculated offset = Remote - Local.
        // process_timing calculates offset = ((Arg2 - Arg1) + (Arg3 - Arg4)) / 2
        // We want ((Remote - Local) + (Remote - Local)) / 2
        // So:
        // Arg1 = Local
        // Arg2 = Remote
        // Arg3 = Remote
        // Arg4 = Local

        self.clock.process_timing(
            ptp_t2_local_rx,  // Arg1 (Local)
            ptp_t1_remote_tx, // Arg2 (Remote)
            ptp_t4_remote_rx, // Arg3 (Remote)
            ptp_t3_local_tx,  // Arg4 (Local)
        );
    }

    /// Check if in sync with group
    #[must_use]
    pub fn is_in_sync(&self) -> bool {
        self.in_sync
    }

    /// Get current group info
    #[must_use]
    pub fn group_info(&self) -> Option<&GroupInfo> {
        self.group.as_ref()
    }

    /// Get clock offset for diagnostics (in ms)
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "Precision loss acceptable for diagnostic display"
    )]
    pub fn clock_offset_ms(&self) -> f64 {
        // PtpClock offset_millis() returns f64
        self.clock.offset_millis()
    }

    /// Helper to convert `Instant` to `PtpTimestamp` using `SystemTime` anchor
    fn instant_to_ptp(instant: Instant) -> PtpTimestamp {
        let now = Instant::now();
        let sys_now = PtpTimestamp::now();

        if instant >= now {
            sys_now.add_duration(instant - now)
        } else {
            let diff = now - instant;
            // PtpTimestamp doesn't have sub_duration, so calculate manually
            // Avoid negative timestamp panic by clamping to zero if somehow wrapped
            let sys_nanos = sys_now.to_nanos();
            // Try to convert diff to i128 nanoseconds
            let diff_nanos = i128::try_from(diff.as_nanos()).unwrap_or(i128::MAX);

            if diff_nanos > sys_nanos {
                PtpTimestamp::ZERO
            } else {
                PtpTimestamp::from_nanos(sys_nanos - diff_nanos)
            }
        }
    }
}

/// Group state for advertisement
impl MultiRoomCoordinator {
    /// Get group UUID for TXT record
    #[must_use]
    pub fn group_uuid(&self) -> Option<&str> {
        self.group.as_ref().map(|g| g.uuid.as_str())
    }

    /// Check if we're the group leader
    #[must_use]
    pub fn is_leader(&self) -> bool {
        self.group
            .as_ref()
            .is_some_and(|g| g.role == GroupRole::Leader)
    }
}
