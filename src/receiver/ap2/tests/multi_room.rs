use crate::protocol::ptp::timestamp::PtpTimestamp;
use crate::receiver::ap2::multi_room::{GroupRole, MultiRoomCoordinator, PlaybackCommand};

#[test]
fn test_new_coordinator() {
    let coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    assert!(coord.group_info().is_none());
    assert!(!coord.is_in_sync());
    assert!(!coord.is_leader());
}

#[test]
fn test_join_leave_group() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));

    let info = coord.group_info().unwrap();
    assert_eq!(info.uuid, "group1");
    assert_eq!(info.role, GroupRole::Follower);
    assert_eq!(info.leader_clock_id, Some(0x5678));
    assert!(!coord.is_leader());

    coord.leave_group();
    assert!(coord.group_info().is_none());
    assert!(!coord.is_leader());
}

#[test]
fn test_leader_role() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Leader, None);

    assert!(coord.is_leader());
}

#[test]
fn test_calculate_adjustment_no_sync() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));
    coord.set_target_time(1000);

    // Not synced yet, so adjustment calculation should return None (not error, just wait)
    let cmd = coord.calculate_adjustment();
    assert!(cmd.is_none());
    assert!(!coord.is_in_sync());
}

#[test]
fn test_calculate_adjustment_in_sync() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));

    // Simulate PTP sync with 0 offset.
    // T1 (Master Tx)
    // T2 (Slave Rx)
    // T3 (Slave Tx)
    // T4 (Master Rx)
    // Offset = 0 if delays are symmetric.
    // Let's use current time as base to ensure we are "recent".
    let now = PtpTimestamp::now();
    let base_sec = now.seconds;

    let t1 = PtpTimestamp::new(base_sec, 0);
    let t2 = PtpTimestamp::new(base_sec, 5_000_000); // +5ms delay
    let t3 = PtpTimestamp::new(base_sec, 10_000_000); // +10ms total
    let t4 = PtpTimestamp::new(base_sec, 15_000_000); // +15ms total

    // Process enough measurements to sync (default min is 1)
    coord.update_timing(t1, t2, t3, t4);

    // Set target time to effectively "now".
    // Since offset is 0, local time == remote time.
    // So if we set target = now, drift should be ~0.
    let now_compact = PtpTimestamp::now().to_airplay_compact();
    coord.set_target_time(now_compact);

    let cmd = coord.calculate_adjustment();

    // Drift should be small (just execution time difference).
    // If it's small, in_sync becomes true and returns None.
    assert!(cmd.is_none());
    // This assertion might be flaky if execution is very slow (>1ms),
    // but usually it should be <1ms.
    // If it fails, we might need to increase tolerance or mock time.
    // For now, let's assume it passes.
}

#[test]
fn test_calculate_adjustment_large_drift() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));

    // Sync with 0 offset
    let now = PtpTimestamp::now();
    let base_sec = now.seconds;
    let t1 = PtpTimestamp::new(base_sec, 0);
    let t2 = PtpTimestamp::new(base_sec, 5_000_000);
    let t3 = PtpTimestamp::new(base_sec, 10_000_000);
    let t4 = PtpTimestamp::new(base_sec, 15_000_000);
    coord.update_timing(t1, t2, t3, t4);

    // Target is way behind (current time is ahead of target)
    // e.g. target = now - 2 seconds.
    let now_compact = PtpTimestamp::now().to_airplay_compact();
    // 2 seconds in compact units (1/65536) = 2 * 65536 = 131072
    let target = now_compact.wrapping_sub(131_072);
    coord.set_target_time(target);

    let cmd = coord.calculate_adjustment();

    // Drift = now - target = +2 seconds.
    // Should be large drift (>10ms).
    // Should return StartAt(target).
    if let Some(PlaybackCommand::StartAt { timestamp }) = cmd {
        assert_eq!(timestamp, target);
    } else {
        panic!("Expected StartAt command, got {cmd:?}");
    }
}

#[test]
fn test_calculate_adjustment_small_drift() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));

    // Sync with 0 offset
    let now = PtpTimestamp::now();
    let base_sec = now.seconds;
    let t1 = PtpTimestamp::new(base_sec, 0);
    let t2 = PtpTimestamp::new(base_sec, 5_000_000);
    let t3 = PtpTimestamp::new(base_sec, 10_000_000);
    let t4 = PtpTimestamp::new(base_sec, 15_000_000);
    coord.update_timing(t1, t2, t3, t4);

    let now_compact = PtpTimestamp::now().to_airplay_compact();
    // Small drift: target = now - 5ms.
    // 5ms in units = 0.005 * 65536 = 327.
    let target = now_compact.wrapping_sub(327);
    coord.set_target_time(target);

    let cmd = coord.calculate_adjustment();

    // Drift = +5ms.
    // Tolerance is 1ms. So it should adjust rate.
    // Rate ppm = drift_us / 10 = 5000 / 10 = 500 ppm.
    if let Some(PlaybackCommand::AdjustRate { rate_ppm }) = cmd {
        assert!(rate_ppm > 0);
        assert!(rate_ppm <= 500);
    } else {
        panic!("Expected AdjustRate command, got {cmd:?}");
    }
}

#[test]
fn test_calculate_adjustment_with_offset() {
    let mut coord = MultiRoomCoordinator::new("device1".to_string(), 0x1234);
    coord.join_group("group1".to_string(), GroupRole::Follower, Some(0x5678));

    // Simulate Slave is 5 seconds ahead of Master.
    // Offset = 5 seconds.
    let now = PtpTimestamp::now();
    let base_sec = now.seconds;

    // T1 (Master Tx) = base
    // T2 (Slave Rx) = base + 5s
    // T3 (Slave Tx) = base + 5s
    // T4 (Master Rx) = base
    let t1 = PtpTimestamp::new(base_sec, 0);
    let t2 = PtpTimestamp::new(base_sec + 5, 0);
    let t3 = PtpTimestamp::new(base_sec + 5, 0);
    let t4 = PtpTimestamp::new(base_sec, 0);

    coord.update_timing(t1, t2, t3, t4);

    // Check offset is calculated correctly (~5s)
    let offset_ms = coord.clock_offset_ms();
    assert!((offset_ms - 5000.0).abs() < 1.0, "Offset was {offset_ms}");

    // Target time is "now" in Master domain.
    // Master = Slave - 5s.
    // So target should be (now - 5s).

    // We need current time again because it advanced slightly.
    let now_fresh = PtpTimestamp::now();
    let now_compact = now_fresh.to_airplay_compact();

    // 5 seconds in compact units = 5 * 65536 = 327680.
    let target = now_compact.wrapping_sub(327_680);

    coord.set_target_time(target);

    let cmd = coord.calculate_adjustment();

    // Drift should be small (Master time derived from Slave time - Offset matches Target).
    // So should be in sync.

    assert!(cmd.is_none(), "Expected no adjustment, got {cmd:?}");
    assert!(coord.is_in_sync());
}
