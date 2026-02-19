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
    /// Member device name
    pub name: String,
    /// Member clock ID
    pub clock_id: u64,
    /// Member role in group
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
#[derive(Debug, Clone)]
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
            // Initialize as Slave by default, can change role later?
            // Usually Receiver is Slave. Even if Leader, it might just use its own clock.
            // But if Leader, it acts as PTP Master?
            // PtpClock has a role.
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
        }
    }

    /// Set target playback time (from sender)
    pub fn set_target_time(&mut self, timestamp: u64) {
        if let Some(ref mut group) = self.group {
            group.target_playback_time = Some(timestamp);
        }
    }

    /// Calculate playback adjustment needed
    #[allow(clippy::cast_possible_truncation)]
    pub fn calculate_adjustment(&mut self) -> Option<PlaybackCommand> {
        let group = self.group.as_ref()?;
        let target = group.target_playback_time?;

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time
        // Use PtpTimestamp::now() which aligns with SystemTime/WallClock used by PtpClock
        let now = PtpTimestamp::now();

        // Convert local time (Slave) to remote time (Master)
        // Offset = Slave - Master
        // Master = Slave - Offset
        let offset = self.clock.offset_nanos();
        let now_nanos = now.to_nanos();
        let master_nanos = now_nanos - offset;

        let current_ptp_ts = if master_nanos >= 0 {
            PtpTimestamp::from_nanos(master_nanos)
        } else {
            PtpTimestamp::ZERO
        };

        let current_ptp = current_ptp_ts.to_airplay_compact();

        // Calculate drift from target
        // current_ptp and target are in 1/65536 seconds units.
        #[allow(clippy::cast_possible_wrap)]
        let diff_units = current_ptp.wrapping_sub(target) as i64;

        // Convert units to nanoseconds: units * 1_000_000_000 / 65536
        // 1_000_000_000 / 65536 = 15258.78...
        // Use integer math:
        let drift_nanos = diff_units * 1_000_000_000 / 65536;
        let drift_micros = drift_nanos / 1000;

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
            // Limit rate adjustment to +/- 500 ppm
            let rate_ppm = (drift_micros / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// Takes four PTP timestamps forming a timing exchange.
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
