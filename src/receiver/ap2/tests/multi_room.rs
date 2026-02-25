use std::time::Instant;

use crate::protocol::ptp::PtpTimestamp;
use crate::receiver::ap2::multi_room::{GroupRole, MultiRoomCoordinator};

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
fn test_timing_flow() {
    let mut coord = MultiRoomCoordinator::new("dev".into(), 123);
    coord.join_group("g".into(), GroupRole::Follower, Some(456));

    // Just ensure it accepts updates without panic
    let now = Instant::now();
    let ptp_val = PtpTimestamp::now().to_airplay_compact();
    coord.update_timing(now, ptp_val, ptp_val, now);

    assert!(!coord.is_in_sync()); // Not enough measurements
}
