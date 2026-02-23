//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use std::time::Instant;

use tracing::{info, warn};

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
    /// Device ID of the member
    pub device_id: String,
    /// Friendly name of the member
    pub name: String,
    /// Clock ID of the member
    pub clock_id: u64,
    /// Role of the member
    pub role: GroupRole,
}

/// Playback timing command
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// Target PTP timestamp (`AirPlay` compact format)
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

/// Multi-room coordinator
pub struct MultiRoomCoordinator {
    /// Our device ID
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
    /// Anchor for converting Instant to `PtpTimestamp`
    /// (Instant, `PtpTimestamp`)
    anchor: (Instant, PtpTimestamp),
}

impl MultiRoomCoordinator {
    /// Create a new multi-room coordinator
    #[must_use]
    pub fn new(device_id: String, clock_id: u64) -> Self {
        Self {
            device_id,
            // Default to Master role initially (self-clocked)
            clock: PtpClock::new(clock_id, PtpRole::Master),
            group: None,
            sync_tolerance_us: 1000, // 1ms default
            last_sync_check: Instant::now(),
            in_sync: false,
            anchor: (Instant::now(), PtpTimestamp::now()),
        }
    }

    /// Get our device ID
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Join a group
    pub fn join_group(&mut self, uuid: String, role: GroupRole, leader_clock_id: Option<u64>) {
        let ptp_role = match role {
            GroupRole::Leader | GroupRole::None => PtpRole::Master,
            GroupRole::Follower => PtpRole::Slave,
        };

        // Re-initialize clock if role changes to ensure clean slate
        if self.clock.role() != ptp_role {
            let my_clock_id = self.clock.clock_id();
            self.clock = PtpClock::new(my_clock_id, ptp_role);
            self.in_sync = false;
        }

        self.group = Some(GroupInfo {
            uuid,
            role,
            leader_clock_id,
            members: Vec::new(),
            target_playback_time: None,
        });

        info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            info!("Left group");
            self.group = None;
            // Revert to Master role (self-clocked)
            if self.clock.role() != PtpRole::Master {
                let my_clock_id = self.clock.clock_id();
                self.clock = PtpClock::new(my_clock_id, PtpRole::Master);
            }
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

        // Only Followers need to adjust to Leader
        if group.role != GroupRole::Follower {
            return None;
        }

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time (converted to Remote/Master time)
        let now = Instant::now();
        let now_ptp = self.instant_to_ptp(now);
        let current_ptp_remote = self.clock.local_to_remote(now_ptp);

        let current_compact = current_ptp_remote.to_airplay_compact();

        // Calculate drift from target
        // (current - target) * (1e9 / 65536) converts 1/65536 units to nanoseconds.
        let diff = i128::from(current_compact) - i128::from(target);
        let drift_ns = diff * 1_000_000_000 / 65536;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Drift in microseconds fits in i64 unless > 290,000 years"
        )]
        let drift_micros = (drift_ns / 1000) as i64;

        self.in_sync = drift_micros.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_micros.abs() > 10_000 {
            // More than 10ms off - hard sync
            warn!(
                "Multi-room: large drift {}us, requesting hard sync",
                drift_micros
            );
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            // Clamp rate adjustment to +/- 500 ppm
            #[allow(clippy::cast_possible_truncation, reason = "Clamped value fits in i32")]
            let rate_ppm = (drift_micros / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// Arguments are expected to be:
    /// t1: Local Send Time (Instant) -> PTP T3 (Slave Send)
    /// t2: Remote Recv Time (u64 compact) -> PTP T4 (Master Recv)
    /// t3: Remote Send Time (u64 compact) -> PTP T1 (Master Send)
    /// t4: Local Recv Time (Instant) -> PTP T2 (Slave Recv)
    pub fn update_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        // Convert Instants to PtpTimestamp (Local)
        let t1_ptp = self.instant_to_ptp(t1);
        let t4_ptp = self.instant_to_ptp(t4);

        // Convert u64 compact to PtpTimestamp (Remote)
        let t2_ptp = PtpTimestamp::from_airplay_compact(t2);
        let t3_ptp = PtpTimestamp::from_airplay_compact(t3);

        // Map to PTP variables for Slave role:
        // PTP T1 (Master Send) = t3
        // PTP T2 (Slave Recv) = t4
        // PTP T3 (Slave Send) = t1
        // PTP T4 (Master Recv) = t2

        self.clock.process_timing(t3_ptp, t4_ptp, t1_ptp, t2_ptp);
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

    /// Get clock offset for diagnostics
    #[must_use]
    pub fn clock_offset_ms(&self) -> f64 {
        self.clock.offset_millis()
    }

    /// Helper to convert Instant to `PtpTimestamp` using anchor
    fn instant_to_ptp(&self, inst: Instant) -> PtpTimestamp {
        let (anchor_inst, anchor_ptp) = self.anchor;
        if inst >= anchor_inst {
            let dur = inst.duration_since(anchor_inst);
            anchor_ptp.add_duration(dur)
        } else {
            // Handle case where inst is before anchor
            let dur = anchor_inst.duration_since(inst);
            #[allow(clippy::cast_possible_wrap)]
            let nanos = anchor_ptp.to_nanos() - dur.as_nanos() as i128;
            if nanos < 0 {
                PtpTimestamp::ZERO
            } else {
                PtpTimestamp::from_nanos(nanos)
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
