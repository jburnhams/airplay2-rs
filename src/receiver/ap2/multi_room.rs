//! Multi-Room Coordination for `AirPlay` 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use crate::protocol::ptp::clock::{PtpClock, PtpRole};
use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::{Duration, Instant, SystemTime};

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
    /// Device ID of member
    pub device_id: String,
    /// Name of member
    pub name: String,
    /// Clock ID of member
    pub clock_id: u64,
    /// Role of member
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
        /// Target timestamp to start/seek to
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
    /// Create a new `MultiRoomCoordinator`
    #[must_use]
    pub fn new(device_id: String, clock_id: u64) -> Self {
        // We use PtpRole::Master as a placeholder because PtpClock implementation
        // assumes Local (Master) -> Remote (Slave) logic for offset calculation.
        // By passing (Local, Remote, Remote, Local) to process_timing, we trick it
        // into calculating Offset = Remote - Local (Master - Slave),
        // so local_to_remote correctly yields Master time.
        Self {
            device_id,
            clock: PtpClock::new(clock_id, PtpRole::Master),
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
    pub fn calculate_adjustment(&mut self) -> Option<PlaybackCommand> {
        let group = self.group.as_ref()?;
        let target = group.target_playback_time?;

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time (Master time)
        let now = Instant::now();
        // local_to_remote returns Master time because we configured PtpClock to calculate
        // Offset = Master - Slave.
        let current_ptp = self.clock.local_to_remote(convert_instant_to_ptp(now));

        // Convert current PTP time to AirPlay compact format (u64)
        // so we can compare with target (which is also u64 compact).
        let current_u64 = current_ptp.to_airplay_compact();

        // AirPlay compact is 48.16 fixed point (units of 1/65536 seconds).
        // Difference is in units of 1/65536 seconds.
        #[allow(
            clippy::cast_possible_wrap,
            reason = "Drift calculation wraps around u64 differences"
        )]
        let diff_units = current_u64.wrapping_sub(target) as i64;

        // Convert to nanoseconds: units * 1_000_000_000 / 65536
        // We use i64::MAX saturation to avoid panics on large drifts
        let drift_ns = match diff_units.checked_mul(1_000_000_000) {
            Some(v) => v / 65536,
            None => i64::MAX, // Saturation
        };
        let drift_micros = drift_ns / 1000;

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
            #[allow(clippy::cast_possible_truncation)]
            let rate_ppm = (drift_micros / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// Arguments match the PTP exchange:
    /// t1: Local Rx Timestamp (Instant) - We received Sync/FollowUp
    /// t2: Remote Origin Timestamp (u64) - Master sent Sync/FollowUp
    /// t3: Remote Rx Timestamp (u64) - Master received `Delay_Req`
    /// t4: Local Tx Timestamp (Instant) - We sent `Delay_Req`
    ///
    /// Note: The doc named them t1, t2, t3, t4 in a specific order.
    /// We pass (Local, Remote, Remote, Local) to `PtpClock::process_timing`
    /// so it calculates Offset = Remote - Local (Master - Slave).
    pub fn update_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        let t1_ptp = convert_instant_to_ptp(t1);
        let t2_ptp = PtpTimestamp::from_airplay_compact(t2);
        let t3_ptp = PtpTimestamp::from_airplay_compact(t3);
        let t4_ptp = convert_instant_to_ptp(t4);

        // We pass (t1, t2, t3, t4) = (Local, Remote, Remote, Local)
        self.clock.process_timing(t1_ptp, t2_ptp, t3_ptp, t4_ptp);
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

/// Helper to convert Instant to `PtpTimestamp` (approximate)
fn convert_instant_to_ptp(instant: Instant) -> PtpTimestamp {
    let now = Instant::now();
    let sys_now = SystemTime::now();

    // Calculate duration between instant and now
    if instant > now {
        // Future instant (shouldn't happen for past measurements, but possible)
        let diff = instant.duration_since(now);
        let sys_time = sys_now + diff;
        sys_time_to_ptp(sys_time)
    } else {
        // Past instant
        let diff = now.duration_since(instant);
        // Handle potential SystemTime underflow (unlikely)
        let sys_time = sys_now - diff;
        sys_time_to_ptp(sys_time)
    }
}

fn sys_time_to_ptp(sys_time: SystemTime) -> PtpTimestamp {
    let dur = sys_time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    PtpTimestamp::new(dur.as_secs(), dur.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_join_leave() {
        let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);

        assert!(coord.group_info().is_none());

        coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x0065_4321));
        assert!(coord.group_info().is_some());
        assert!(!coord.is_leader());

        coord.leave_group();
        assert!(coord.group_info().is_none());
    }

    #[test]
    fn test_leader_role() {
        let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);
        coord.join_group("group-uuid".into(), GroupRole::Leader, None);

        assert!(coord.is_leader());
    }

    #[test]
    fn test_drift_calculation() {
        let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);

        // Mock sync state
        // We need to inject measurements so the clock becomes synchronized.
        // We simulate a perfect sync first (Offset = 0).
        let now = Instant::now();
        let t1 = now;
        let t2 = PtpTimestamp::now().to_airplay_compact(); // Remote = Local
        let t3 = t2;
        let t4 = now;

        coord.update_timing(t1, t2, t3, t4);

        // Need min_sync_measurements (default 1).
        assert!(coord.clock.is_synchronized());

        // Set target time slightly behind current time (drift positive)
        let current_ptp = coord
            .clock
            .local_to_remote(convert_instant_to_ptp(Instant::now()));
        let current_u64 = current_ptp.to_airplay_compact();

        // Target is 50ms behind (we are ahead by 50ms)
        // 50ms = 0.05s. In compact units: 0.05 * 65536 = 3276.
        let target = current_u64 - 3276;

        coord.join_group("uuid".into(), GroupRole::Follower, None);
        coord.set_target_time(target);

        let cmd = coord.calculate_adjustment();

        // Drift is ~50ms = 50,000us.
        // > 10,000us -> StartAt
        match cmd {
            Some(PlaybackCommand::StartAt { timestamp }) => {
                // Approximate comparison
                let diff = timestamp.abs_diff(target);
                assert!(diff < 100);
            }
            _ => panic!("Expected StartAt command, got {cmd:?}"),
        }
    }
}
