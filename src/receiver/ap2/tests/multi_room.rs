use crate::protocol::ptp::PtpTimestamp;
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
    assert_eq!(coord.group_info().unwrap().role, GroupRole::Leader);
}

#[test]
fn test_calculate_adjustment_no_sync() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);
    coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x0065_4321));

    // Set target time
    let target = PtpTimestamp::now().to_airplay_compact();
    coord.set_target_time(target);

    // Should return None because clock is not synchronized
    assert!(coord.calculate_adjustment().is_none());
}

#[test]
fn test_calculate_adjustment_synced() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);
    coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x0065_4321));

    // Simulate synchronization by processing timing measurements
    // We simulate a perfect network with 0 delay and 0 offset
    let t = PtpTimestamp::now();
    // T1 (Master Send) = T
    // T2 (Slave Recv) = T
    // T3 (Slave Send) = T + 1s
    // T4 (Master Recv) = T + 1s
    let t1 = t;
    let t2 = t;
    let t3 = t.add_duration(std::time::Duration::from_secs(1));
    let t4 = t.add_duration(std::time::Duration::from_secs(1));

    // Process enough measurements to sync (min_sync_measurements is default 1)
    coord.update_timing(t1, t2, t3, t4);

    // Now we are synced (offset should be 0)

    // 1. Test perfectly in sync
    let now = PtpTimestamp::now();
    // Target is 'now' in compact format
    let target = now.to_airplay_compact();
    coord.set_target_time(target);

    // Since we are checking against 'now' inside calculate_adjustment,
    // and 'now' moves forward, we might have slight drift.
    // However, PtpTimestamp::now() is called inside.
    // If we want deterministic test, we'd need to mock time or inject it.
    // For now, let's assume it runs fast enough that drift is small (< 1ms tolerance).
    // Or we can just check it returns *something* or None if perfect.

    // If we set target to now, and calculate_adjustment calls now() immediately,
    // drift should be very small (~0).
    // So it should return None (in sync).
    assert!(coord.calculate_adjustment().is_none());

    // 2. Test large drift (target is far in future -> we are behind -> should jump?)
    // Or target is far in past -> we are ahead.
    // If target = now + 10s.
    // current_ptp (now) - target = -10s.
    // drift is -10s.
    // abs(drift) > 10ms.
    // Should return StartAt(target).

    let future_target = now
        .add_duration(std::time::Duration::from_secs(10))
        .to_airplay_compact();
    coord.set_target_time(future_target);

    match coord.calculate_adjustment() {
        Some(PlaybackCommand::StartAt { timestamp }) => {
            assert_eq!(timestamp, future_target);
        }
        _ => panic!("Expected StartAt for large drift"),
    }
}
