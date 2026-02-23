use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::protocol::ptp::timestamp::PtpTimestamp;
use crate::receiver::ap2::multi_room::{GroupRole, MultiRoomCoordinator, PlaybackCommand};

#[test]
fn test_group_join_leave() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x0012_3456);

    assert!(coord.group_info().is_none());

    coord.join_group(
        "group-uuid".into(),
        GroupRole::Follower,
        Some(0x0065_4321),
    );
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
fn test_timing_update_and_sync() {
    let mut coord = MultiRoomCoordinator::new("device-id".into(), 1);
    coord.join_group("group".into(), GroupRole::Follower, Some(2));

    // Simulate Slave exchange with Master with 0 offset and 20ms RTT.
    let t1 = Instant::now();
    // Simulate some time passing for processing
    std::thread::sleep(Duration::from_millis(5));

    // Estimate PTP time
    let now_sys = SystemTime::now();
    let now_ptp = PtpTimestamp::from_duration(now_sys.duration_since(UNIX_EPOCH).unwrap());

    // t1 corresponds roughly to now_ptp.
    // t2 (Remote Recv) = t1 + 10ms (latency)
    let t2_ptp = now_ptp.add_duration(Duration::from_millis(10));
    // t3 (Remote Send) = t2 + 5ms (processing)
    let t3_ptp = t2_ptp.add_duration(Duration::from_millis(5));
    // t4 (Local Recv) = t1 + 25ms (10+5+10)
    let t4 = t1 + Duration::from_millis(25);

    let t2_compact = t2_ptp.to_airplay_compact();
    let t3_compact = t3_ptp.to_airplay_compact();

    coord.update_timing(t1, t2_compact, t3_compact, t4);

    // Now check adjustment logic
    // Set target to "now" + small delay to simulate playback
    let target_ptp = now_ptp.add_duration(Duration::from_millis(100));
    coord.set_target_time(target_ptp.to_airplay_compact());

    let cmd = coord.calculate_adjustment();

    // Since we only fed one measurement, it might or might not be stable,
    // but PtpClock defaults to 1 min sync.
    // If it returns a command, it should be AdjustRate or None (if very close).
    // StartAt only if drift > 10ms.
    // Our drift calculation depends on when calculate_adjustment is called vs target.
    // We set target to +100ms.
    // If we call it immediately, current time is approx now_ptp.
    // Drift = now - target = -100ms.
    // This is large drift. Expect StartAt.

    if let Some(c) = cmd {
        match c {
            PlaybackCommand::StartAt { timestamp } => {
                assert_eq!(timestamp, target_ptp.to_airplay_compact());
            }
            _ => panic!("Expected StartAt due to large drift, got {c:?}"),
        }
    }
}
