use std::time::{Duration, Instant};

use crate::protocol::ptp::timestamp::PtpTimestamp;
use crate::receiver::ap2::multi_room::{GroupRole, MultiRoomCoordinator, PlaybackCommand};

#[test]
fn test_group_join_leave() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);

    assert!(coord.group_info().is_none());

    coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x0065_4321));
    assert!(coord.group_info().is_some());
    assert!(!coord.is_leader());
    assert_eq!(coord.group_uuid(), Some("group-uuid"));

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
fn test_adjustment_no_sync() {
    let mut coord = MultiRoomCoordinator::new("dev".into(), 1);
    coord.join_group("grp".into(), GroupRole::Follower, Some(2));
    coord.set_target_time(1000);

    // Not synced yet
    assert!(coord.calculate_adjustment().is_none());
}

#[test]
fn test_adjustment_synced() {
    let mut coord = MultiRoomCoordinator::new("dev".into(), 1);
    coord.join_group("grp".into(), GroupRole::Follower, Some(2));

    // Simulate perfect sync
    // Get current time as PTP timestamp
    let now_ptp = PtpTimestamp::now();
    let now_compact = now_ptp.to_airplay_compact();
    let now_inst = Instant::now();

    // Feed measurements to sync clock
    coord.update_timing(now_compact, now_inst, now_inst, now_compact);

    // Set target to exactly now
    coord.set_target_time(now_compact);

    // Calculate adjustment
    let cmd = coord.calculate_adjustment();

    if let Some(PlaybackCommand::StartAt { .. }) = cmd {
        panic!("Should not require hard sync with 0 offset");
    } else {
        // Either None (in sync) or AdjustRate (small drift) is acceptable
    }
}

#[test]
fn test_adjustment_with_offset() {
    let mut coord = MultiRoomCoordinator::new("dev".into(), 1);
    coord.join_group("grp".into(), GroupRole::Follower, Some(2));

    // Simulate Slave clock ahead by 100ms
    // Offset = Slave - Master = +100ms.

    let now_inst = Instant::now();
    let now_ptp = PtpTimestamp::now();
    let offset_dur = Duration::from_millis(100);

    // Calculate Master Time = now_ptp - offset
    let master_time_ptp =
        PtpTimestamp::from_duration(now_ptp.to_duration().checked_sub(offset_dur).unwrap());
    let master_compact = master_time_ptp.to_airplay_compact();

    // Feed measurements:
    // t1 (Master Tx) = master
    // t2 (Slave Rx) = now
    // t3 (Slave Tx) = now
    // t4 (Master Rx) = master
    // RTT = 0. Offset = 100ms.
    coord.update_timing(master_compact, now_inst, now_inst, master_compact);

    // Check offset is approx 100ms
    let offset_ms = coord.clock_offset_ms();
    assert!(
        (offset_ms - 100.0).abs() < 5.0,
        "Offset should be approx 100ms, got {offset_ms}"
    );

    // Set target to exactly Master Time
    coord.set_target_time(master_compact);

    // Should be synced (drift ~ 0)
    let cmd = coord.calculate_adjustment();
    if let Some(PlaybackCommand::StartAt { .. }) = cmd {
        panic!("Should be synced when target is adjusted for offset");
    }

    // Set target to Slave Time (which is Master + 100ms)
    // Current Master is T. Target is T + 100ms.
    // Drift = T - (T + 100) = -100ms.
    // Should trigger StartAt due to > 10ms drift.
    let slave_compact = now_ptp.to_airplay_compact();
    coord.set_target_time(slave_compact);

    let cmd = coord.calculate_adjustment();
    if let Some(PlaybackCommand::StartAt { timestamp }) = cmd {
        assert_eq!(timestamp, slave_compact);
    } else {
        panic!("Should detect large drift (-100ms) and StartAt");
    }
}
