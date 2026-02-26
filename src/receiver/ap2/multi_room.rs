//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use crate::protocol::ptp::clock::{PtpClock, PtpRole};
use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Group role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupRole {
    /// Not in a group
    #[default]
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
    /// Device ID
    pub device_id: String,
    /// Device name
    pub name: String,
    /// Clock ID
    pub clock_id: u64,
    /// Group role
    pub role: GroupRole,
}

/// Playback timing command
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// PTP timestamp to start playback
        timestamp: u64
    },
    /// Adjust playback rate to catch up/slow down
    AdjustRate {
        /// Adjustment rate in parts per million
        rate_ppm: i32
    },
    /// Pause playback
    Pause,
    /// Resume playback
    Resume,
}

/// Multi-room coordinator
pub struct MultiRoomCoordinator {
    /// Our device ID
    #[allow(dead_code, reason = "Reserved for future use")]
    device_id: String,
    /// Our clock
    clock: PtpClock,
    /// Current group info
    group: Option<GroupInfo>,
    /// Sync tolerance (microseconds)
    sync_tolerance_us: i64,
    /// Last sync check
    #[allow(dead_code, reason = "Reserved for rate limiting checks")]
    last_sync_check: Instant,
    /// Sync status
    in_sync: bool,
}

impl MultiRoomCoordinator {
    /// Create a new multi-room coordinator.
    #[must_use]
    pub fn new(device_id: String, clock_id: u64) -> Self {
        Self {
            device_id,
            // Initialize as Slave by default since we are a receiver usually syncing to a master
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

        info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            info!("Left group");
            self.group = None;
            self.clock.reset(); // Reset PTP state
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

        // Get current position in PTP time (Master time)
        // Since we are Slave (Follower), we convert our local time to Remote (Master) time.
        // PtpClock implementation defines "Local" as Master and "Remote" as Slave for offset calculation:
        // offset = Slave - Master.
        // remote_to_local(ts) = ts - offset = ts - (Slave - Master).
        // If we pass Slave time as input: Slave - (Slave - Master) = Master.
        // So we use remote_to_local to convert Slave -> Master.
        let now = PtpTimestamp::now(); // System time (Slave/Local)
        let master_time = self.clock.remote_to_local(now);
        let current_ptp = master_time.to_airplay_compact();

        // Calculate drift from target
        // drift = current_ptp - target

        #[allow(clippy::cast_lossless, reason = "u64 fits in i128")]
        let current_ptp_i128 = i128::from(current_ptp);
        #[allow(clippy::cast_lossless, reason = "u64 fits in i128")]
        let target_i128 = i128::from(target);

        // 1/65536 sec units to nanoseconds: * 1_000_000_000 / 65536
        let drift_ns = (current_ptp_i128 - target_i128) * 1_000_000_000 / 65536;

        #[allow(clippy::cast_possible_truncation, reason = "drift fits in i64 unless huge")]
        let drift_micros = (drift_ns / 1000) as i64;

        self.in_sync = drift_micros.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_micros.abs() > 10_000 {
            // More than 10ms off - hard sync
            warn!("Multi-room: large drift {}us, requesting hard sync", drift_micros);
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            // 500 ppm max adjustment
            #[allow(clippy::cast_possible_truncation, reason = "clamped value fits in i32")]
            let rate_ppm = (drift_micros / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// t1: Master Send Time (Compact u64)
    /// t2: Slave Receive Time (Local Instant)
    /// t3: Slave Send Time (Local Instant)
    /// t4: Master Receive Time (Compact u64)
    pub fn update_timing(&mut self, t1_compact: u64, t2_local: Instant, t3_local: Instant, t4_compact: u64) {
        let t1 = PtpTimestamp::from_airplay_compact(t1_compact);
        let t2 = Self::instant_to_ptp(t2_local);
        let t3 = Self::instant_to_ptp(t3_local);
        let t4 = PtpTimestamp::from_airplay_compact(t4_compact);

        self.clock.process_timing(t1, t2, t3, t4);
    }

    /// Helper to convert Instant to `PtpTimestamp`
    fn instant_to_ptp(inst: Instant) -> PtpTimestamp {
        let now_inst = Instant::now();
        let now_sys = SystemTime::now();

        // Calculate duration difference
        if inst > now_inst {
            let dur = inst - now_inst;
            let target_sys = now_sys + dur;
            let dur_since_epoch = target_sys.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
            PtpTimestamp::from_duration(dur_since_epoch)
        } else {
            let dur = now_inst - inst;
            let target_sys = now_sys.checked_sub(dur).unwrap_or(UNIX_EPOCH);
            let dur_since_epoch = target_sys.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
            PtpTimestamp::from_duration(dur_since_epoch)
        }
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
