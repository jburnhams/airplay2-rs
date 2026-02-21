use crate::protocol::ptp::timestamp::PtpTimestamp;
use crate::receiver::ap2::multi_room::{GroupRole, MultiRoomCoordinator, PlaybackCommand};

#[test]
fn test_group_join_leave() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x123456);

    assert!(coord.group_info().is_none());
    assert!(!coord.is_leader());

    coord.join_group("group-uuid".into(), GroupRole::Follower, Some(0x654321));
    assert!(coord.group_info().is_some());
    assert!(!coord.is_leader());
    assert_eq!(coord.group_uuid(), Some("group-uuid"));

    let info = coord.group_info().unwrap();
    assert_eq!(info.role, GroupRole::Follower);
    assert_eq!(info.leader_clock_id, Some(0x654321));

    coord.leave_group();
    assert!(coord.group_info().is_none());
}

#[test]
fn test_leader_role() {
    let mut coord = MultiRoomCoordinator::new("AA:BB:CC:DD:EE:FF".into(), 0x123456);
    coord.join_group("group-uuid".into(), GroupRole::Leader, None);

    assert!(coord.is_leader());

    // Leader should not calculate adjustment
    coord.set_target_time(100);
    assert!(coord.calculate_adjustment().is_none());
}

#[test]
fn test_timing_update_and_sync() {
    let mut coord = MultiRoomCoordinator::new("dev-id".into(), 0x1234);
    coord.join_group("group".into(), GroupRole::Follower, Some(0x5678));

    // Initially not in sync
    assert!(!coord.is_in_sync());

    // Feed timing updates to synchronize PtpClock
    // Master: 100s. Slave: 105s. Offset = Slave - Master = 5s.
    // T1 (Master) = 100.
    // T2 (Slave) = 105.001.
    // T3 (Slave) = 105.002.
    // T4 (Master) = 100.003.
    // Offset = 5s.

    // Unused but kept for reference on expected structure
    let t1_base = PtpTimestamp::new(100, 0);
    let _ = t1_base.to_airplay_compact();
    let _ = PtpTimestamp::new(100, 3_000_000).to_airplay_compact();

    for i in 0..10 {
        // Shift timestamps slightly to simulate progression
        let shift = (i * 1_000_000) as u32; // 1ms steps
        let t1 = PtpTimestamp::new(100, shift).to_airplay_compact();
        let t2 = PtpTimestamp::new(105, 1_000_000 + shift);
        let t3 = PtpTimestamp::new(105, 2_000_000 + shift);
        let t4 = PtpTimestamp::new(100, 3_000_000 + shift).to_airplay_compact();

        coord.update_timing(t1, t2, t3, t4);
    }

    // Now clock should be synced.
    // Offset is ~5000ms.
    assert!((coord.clock_offset_ms() - 5000.0).abs() < 10.0);
}

#[test]
fn test_adjustment_calculation_small_drift() {
    let mut coord = MultiRoomCoordinator::new("dev-id".into(), 0x1234);
    coord.join_group("group".into(), GroupRole::Follower, Some(0x5678));

    // Sync the clock with 0 offset for simplicity
    for i in 0..10 {
        let shift = (i * 1_000_000) as u32;
        let t1 = PtpTimestamp::new(100, shift).to_airplay_compact();
        let t2 = PtpTimestamp::new(100, 1_000_000 + shift); // Offset 0 (approx 0.5ms delay one way)
        let t3 = PtpTimestamp::new(100, 2_000_000 + shift);
        let t4 = PtpTimestamp::new(100, 3_000_000 + shift).to_airplay_compact();

        coord.update_timing(t1, t2, t3, t4);
    }

    // Now synced.
    // `calculate_adjustment` compares `current_ptp` (Local converted to Master) with `target`.
    // Since offset is 0, Local ~ Master.
    // `current_ptp` ~ `now`.

    // We can't easily control `PtpTimestamp::now()` as it uses `SystemTime`.
    // But `calculate_adjustment` reads `now()`.

    // To test `calculate_adjustment`, we need to set `target` relative to `now`.

    let _ = PtpTimestamp::now();
    // Removed unused target_ts calculation

    // Wait, PtpTimestamp doesn't have `sub_duration`.
    // `current_ptp` will be `now` (since offset 0).
    // If we set `target` to `now - 1ms`.
    // Drift = `now` - `(now - 1ms)` = +1ms.

    // Since we can't inject time into `MultiRoomCoordinator` (it calls `PtpTimestamp::now()`),
    // we have to rely on relative timing.

    // Strategy: Read `now` immediately before setting target.
    let now_ts = PtpTimestamp::now();
    // Set target 2ms in the PAST.
    // `current` will be >= `now_ts`.
    // `drift` >= 2ms.

    // target = now - 2ms.
    // We can construct target manually.
    #[allow(clippy::cast_possible_truncation)]
    let target_nanos = now_ts.to_nanos() - 2_000_000;
    let target_ts = PtpTimestamp::from_nanos(target_nanos);

    coord.set_target_time(target_ts.to_airplay_compact());

    // Calculate adjustment.
    // Drift ~ 2000us.
    // Rate ~ 200ppm.
    let adj = coord.calculate_adjustment();

    match adj {
        Some(PlaybackCommand::AdjustRate { rate_ppm }) => {
            // Allow some jitter, but should be positive and around 200.
            assert!(
                rate_ppm > 100 && rate_ppm < 300,
                "Rate ppm {rate_ppm} not in range"
            );
        }
        _ => panic!("Expected AdjustRate, got {:?}", adj),
    }
}

#[test]
fn test_adjustment_calculation_large_drift() {
    let mut coord = MultiRoomCoordinator::new("dev-id".into(), 0x1234);
    coord.join_group("group".into(), GroupRole::Follower, Some(0x5678));

    // Sync clock (offset 0)
    for i in 0..10 {
        let shift = (i * 1_000_000) as u32;
        let t1 = PtpTimestamp::new(100, shift).to_airplay_compact();
        let t2 = PtpTimestamp::new(100, 1_000_000 + shift);
        let t3 = PtpTimestamp::new(100, 2_000_000 + shift);
        let t4 = PtpTimestamp::new(100, 3_000_000 + shift).to_airplay_compact();
        coord.update_timing(t1, t2, t3, t4);
    }

    let now_ts = PtpTimestamp::now();
    // Set target 1 second in the PAST.
    // Drift = 1s = 1_000_000 us.
    // Should trigger StartAt (Hard Sync).

    let target_nanos = now_ts.to_nanos() - 1_000_000_000;
    let target_ts = PtpTimestamp::from_nanos(target_nanos);

    coord.set_target_time(target_ts.to_airplay_compact());

    let adj = coord.calculate_adjustment();
    match adj {
        Some(PlaybackCommand::StartAt { timestamp }) => {
            assert_eq!(timestamp, target_ts.to_airplay_compact());
        }
        _ => panic!("Expected StartAt, got {:?}", adj),
    }
}
