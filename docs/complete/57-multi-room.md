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

use crate::protocol::ptp::clock::{PtpClock, PtpRole};
use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::Instant;
use tracing::{info, warn};

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
    /// Member name
    pub name: String,
    /// Member clock ID
    pub clock_id: u64,
    /// Member role
    pub role: GroupRole,
}

/// Multi-room coordinator
pub struct MultiRoomCoordinator {
    /// Our device ID
    #[allow(dead_code, reason = "Reserved for future identification usage")]
    device_id: String,
    /// Our clock
    clock: PtpClock,
    /// Current group info
    group: Option<GroupInfo>,
    /// Sync tolerance (microseconds)
    sync_tolerance_us: i64,
    /// Last sync check
    #[allow(dead_code, reason = "Reserved for future sync timeout handling")]
    last_sync_check: Instant,
    /// Sync status
    in_sync: bool,
}

/// Playback timing command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackCommand {
    /// Start playback at specified time
    StartAt {
        /// Target PTP timestamp (AirPlay compact format)
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
    pub fn new(device_id: String, clock_id: u64) -> Self {
        Self {
            device_id,
            clock: PtpClock::new(clock_id, PtpRole::Slave), // Default role, adjusted if we become Leader
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

        info!(role = ?role, "Joined multi-room group");
    }

    /// Leave current group
    pub fn leave_group(&mut self) {
        if self.group.is_some() {
            info!("Left multi-room group");
            self.group = None;
            self.clock.reset(); // Clear synchronization state
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

        // If we are Leader, we don't adjust?
        if group.role == GroupRole::Leader {
            return None;
        }

        let target = group.target_playback_time?;

        if !self.clock.is_synchronized() {
            return None;
        }

        // Get current position in PTP time (Remote/Master time)
        let now = PtpTimestamp::now(); // Local time
        let current_ptp = self.clock.local_to_remote(now); // Converted to Remote (Master) time

        // Target is in Remote (Master) time (AirPlay compact format)
        let target_ptp = PtpTimestamp::from_airplay_compact(target);

        let drift_ns = current_ptp.diff_nanos(&target_ptp);

        // Convert to microseconds with saturation to avoid panic on huge drifts
        let drift_us = i64::try_from(drift_ns / 1000).unwrap_or(if drift_ns > 0 { i64::MAX } else { i64::MIN });

        self.in_sync = drift_us.abs() < self.sync_tolerance_us;

        if self.in_sync {
            return None;
        }

        // Need adjustment
        if drift_us.abs() > 10_000 {
            // More than 10ms off - hard sync
            warn!(drift_us, "Multi-room: large drift, requesting hard sync");
            Some(PlaybackCommand::StartAt { timestamp: target })
        } else {
            // Small drift - adjust rate
            #[allow(clippy::cast_possible_truncation)]
            let rate_ppm = (drift_us / 10).clamp(-500, 500) as i32;
            Some(PlaybackCommand::AdjustRate { rate_ppm })
        }
    }

    /// Process timing update
    ///
    /// t1: Master send time (AirPlay timestamp)
    /// t2: Slave receive time (Local PTP timestamp)
    /// t3: Slave send time (Local PTP timestamp)
    /// t4: Master receive time (AirPlay timestamp)
    pub fn update_timing(
        &mut self,
        t1: u64,
        t2: PtpTimestamp,
        t3: PtpTimestamp,
        t4: u64,
    ) {
        // Convert AirPlay timestamps to PtpTimestamp
        let t1_ts = PtpTimestamp::from_airplay_compact(t1);
        let t4_ts = PtpTimestamp::from_airplay_compact(t4);

        self.clock.process_timing(t1_ts, t2, t3, t4_ts);
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
        self.clock.offset_millis()
    }

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
