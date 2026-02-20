//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use crate::protocol::ptp::{PtpClock, PtpRole, PtpTimestamp};
use std::time::Instant;

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
    /// Human-readable name
    pub name: String,
    /// PTP Clock Identity
    pub clock_id: u64,
    /// Role in the group
    pub role: GroupRole,
}

/// Multi-room coordinator
#[derive(Debug)]
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
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// The timestamp to start playback at (`AirPlay` compact format)
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
            // Initialize as Slave by default; role will be updated on join_group
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

        // Update PTP role if needed (though PtpClock currently takes role in constructor,
        // ideally it might support changing role, but here we just note the group role)
        // If we became Leader, we act as PTP Master for others, but PtpClock logic is same.
        // However, PtpClock struct has `role` field. It's immutable in current impl.
        // For now, we assume PtpClock is mainly used for synchronization logic (Slave).
        // If we are Leader, we are the reference, so offset should be 0.

        tracing::info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            tracing::info!("Left group");
            self.group = None;
            self.clock.reset(); // Reset sync state
        }
    }

    /// Set target playback time (from sender)
    pub fn set_target_time(&mut self, timestamp: u64) {
        if let Some(ref mut group) = self.group {
            group.target_playback_time = Some(timestamp);
        }
    }

    /// Calculate playback adjustment needed
    #[allow(
        clippy::similar_names,
        reason = "Using ns/us suffixes for standard units"
    )]
    pub fn calculate_adjustment(&mut self) -> Option<PlaybackCommand> {
        let group = self.group.as_ref()?;
        let target = group.target_playback_time?;

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time
        let now = PtpTimestamp::now();
        let current_ptp = self.clock.local_to_remote(now);

        // Convert PTP timestamp to AirPlay compact format (u64)
        // Note: target is likely in AirPlay compact format (48.16 fixed point)
        // current_ptp.to_airplay_compact() returns u64 in same format.
        let current_compact = current_ptp.to_airplay_compact();

        // Calculate drift from target
        // Both are u64, but we need signed difference.
        // The difference is in units of 1/65536 seconds.
        #[allow(
            clippy::cast_possible_wrap,
            reason = "AirPlay compact timestamps fit in i64"
        )]
        let drift_units = current_compact as i64 - target as i64;

        // Convert to nanoseconds: units * 1_000_000_000 / 65536
        // We use i128 to prevent overflow during multiplication
        let drift_ns_i128 = (i128::from(drift_units) * 1_000_000_000) / 65536;

        #[allow(clippy::cast_possible_truncation, reason = "Drift in ns fits in i64 unless extremely large")]
        let drift_ns = drift_ns_i128 as i64;
        let drift_us = drift_ns / 1000;

        self.in_sync = drift_us.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_us.abs() > 10_000 {
            // More than 10ms off - hard sync
            tracing::warn!(
                "Multi-room: large drift {}us, requesting hard sync",
                drift_us
            );
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Clamped value fits in i32"
            )]
            let rate_ppm = i32::try_from((drift_us / 10).clamp(-500, 500)).unwrap_or(0);
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update with PTP timestamps
    /// T1: Master send time (remote)
    /// T2: Slave receive time (local)
    /// T3: Slave send time (local)
    /// T4: Master receive time (remote)
    pub fn update_timing(
        &mut self,
        t1: PtpTimestamp,
        t2: PtpTimestamp,
        t3: PtpTimestamp,
        t4: PtpTimestamp,
    ) {
        self.clock.process_timing(t1, t2, t3, t4);
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
