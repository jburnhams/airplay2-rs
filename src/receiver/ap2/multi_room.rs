//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use crate::protocol::ptp::{PtpClock, PtpRole, PtpTimestamp};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    /// Member device ID
    pub device_id: String,
    /// Member name
    pub name: String,
    /// Member clock ID
    pub clock_id: u64,
    /// Member role
    pub role: GroupRole,
}

/// Playback timing command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// Target PTP timestamp
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
    #[allow(dead_code)]
    device_id: String,
    /// Our clock
    clock: PtpClock,
    /// Current group info
    group: Option<GroupInfo>,
    /// Sync tolerance (microseconds)
    sync_tolerance_micros: i64,
    /// Last sync check
    #[allow(dead_code)]
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
            clock: PtpClock::new(clock_id, PtpRole::Slave),
            group: None,
            sync_tolerance_micros: 1000, // 1ms default
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

        let ptp_role = match role {
            GroupRole::Leader => PtpRole::Master,
            GroupRole::Follower | GroupRole::None => PtpRole::Slave,
        };
        let clock_id = self.clock.clock_id();
        self.clock = PtpClock::new(clock_id, ptp_role);

        tracing::info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            tracing::info!("Left group");
            self.group = None;
            let clock_id = self.clock.clock_id();
            self.clock = PtpClock::new(clock_id, PtpRole::Slave);
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

        let now = Instant::now();
        let ptp_now = Self::instant_to_ptp(now);
        let current_ptp = self.clock.local_to_remote(ptp_now);

        let target_ptp = PtpTimestamp::from_airplay_compact(target);
        let drift_nanos = current_ptp.diff_nanos(&target_ptp);
        let drift_micros = drift_nanos / 1000;

        #[allow(
            clippy::cast_possible_truncation,
            reason = "Drift is small enough for i64"
        )]
        let drift_micros_i64 = drift_micros as i64;

        self.in_sync = drift_micros_i64.abs() < self.sync_tolerance_micros;

        // Update last sync check time
        self.last_sync_check = now;

        if self.in_sync {
            return None;
        }

        if drift_micros_i64.abs() > 10_000 {
            tracing::warn!(
                "Multi-room: large drift {}us, requesting hard sync",
                drift_micros_i64
            );
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Safe because we clamp to -500..500
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Clamped to i32 range"
            )]
            let rate_ppm = (drift_micros_i64 / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    pub fn update_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        let ptp_t1 = Self::instant_to_ptp(t1);
        let ptp_t2 = PtpTimestamp::from_airplay_compact(t2);
        let ptp_t3 = PtpTimestamp::from_airplay_compact(t3);
        let ptp_t4 = Self::instant_to_ptp(t4);

        self.clock.process_timing(ptp_t1, ptp_t2, ptp_t3, ptp_t4);
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

    fn instant_to_ptp(instant: Instant) -> PtpTimestamp {
        let now = Instant::now();
        let sys_now = SystemTime::now();

        let dur_since_epoch = if instant > now {
            let diff = instant - now;
            sys_now.checked_add(diff).unwrap_or(sys_now)
        } else {
            let diff = now - instant;
            sys_now.checked_sub(diff).unwrap_or(sys_now)
        }
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);

        PtpTimestamp::from_duration(dur_since_epoch)
    }
}
