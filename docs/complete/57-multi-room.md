# Section 57: Multi-Room Coordination

## Dependencies
- **Section 55**: PTP Timing Synchronization
- **Section 56**: Buffering & Jitter Management
- **Section 47**: Service Advertisement (feature bit 38)

## Overview

Multi-room audio allows synchronized playback across multiple AirPlay 2 receivers. This requires precise timing coordination using PTP, larger audio buffers, and group management.

Feature bit 38 (SupportsBufferedAudio) enables multi-room support.

## Objectives

- Coordinate playback timing across group members
- Support group leader/follower roles
- Handle group join/leave
- Maintain synchronization within acceptable tolerance

---

## Tasks

### 57.1 Multi-Room Coordinator

**File:** `src/receiver/ap2/multi_room.rs`

```rust
//! Multi-Room Coordination for AirPlay 2
//!
//! Enables synchronized playback across multiple receivers in a group.

use super::ptp_clock::PtpClock;
use std::time::{Duration, Instant};
use std::collections::HashMap;

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
    pub device_id: String,
    pub name: String,
    pub clock_id: u64,
    pub role: GroupRole,
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
    last_sync_check: Instant,
    /// Sync status
    in_sync: bool,
}

/// Playback timing command
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt { timestamp: u64 },
    /// Adjust playback rate to catch up/slow down
    AdjustRate { rate_ppm: i32 },
    /// Pause playback
    Pause,
    /// Resume playback
    Resume,
}

impl MultiRoomCoordinator {
    pub fn new(device_id: String, clock_id: u64) -> Self {
        Self {
            device_id,
            clock: PtpClock::new(clock_id),
            group: None,
            sync_tolerance_us: 1000,  // 1ms default
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

        log::info!("Joined group as {:?}", role);
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            log::info!("Left group");
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

        // Get current position in PTP time
        let now = Instant::now();
        let current_ptp = self.clock.local_to_remote(now);

        // Calculate drift from target
        let drift_ns = (current_ptp as i64 - target as i64) * (1_000_000_000 / 65536);
        let drift_us = drift_ns / 1000;

        self.in_sync = drift_us.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_us.abs() > 10_000 {
            // More than 10ms off - hard sync
            log::warn!("Multi-room: large drift {}us, requesting hard sync", drift_us);
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            let rate_ppm = (drift_us / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    pub fn update_timing(&mut self, t1: Instant, t2: u64, t3: u64, t4: Instant) {
        self.clock.process_timing(t1, t2, t3, t4);
    }

    /// Check if in sync with group
    pub fn is_in_sync(&self) -> bool {
        self.in_sync
    }

    /// Get current group info
    pub fn group_info(&self) -> Option<&GroupInfo> {
        self.group.as_ref()
    }

    /// Get clock offset for diagnostics
    pub fn clock_offset_ms(&self) -> f64 {
        self.clock.offset_ms()
    }
}

/// Group state for advertisement
impl MultiRoomCoordinator {
    /// Get group UUID for TXT record
    pub fn group_uuid(&self) -> Option<&str> {
        self.group.as_ref().map(|g| g.uuid.as_str())
    }

    /// Check if we're the group leader
    pub fn is_leader(&self) -> bool {
        self.group.as_ref()
            .map(|g| g.role == GroupRole::Leader)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_join_leave() {
        let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x123456);

        assert!(coord.group_info().is_none());

        coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x654321));
        assert!(coord.group_info().is_some());
        assert!(!coord.is_leader());

        coord.leave_group();
        assert!(coord.group_info().is_none());
    }

    #[test]
    fn test_leader_role() {
        let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x123456);
        coord.join_group("group-uuid".into(), GroupRole::Leader, None);

        assert!(coord.is_leader());
    }
}
```

---

## Acceptance Criteria

- [x] Group join/leave functionality
- [x] Leader/follower role support
- [x] Playback time synchronization
- [x] Drift detection and correction
- [x] Clock offset tracking
- [x] All unit tests pass

---

## References

- [AirPlay 2 Multi-Room](https://www.apple.com/airplay/)
- [Section 55: PTP Timing](./55-ptp-timing.md)
