use crate::protocol::ptp::clock::{PtpClock, PtpRole, TimingMeasurement};
use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::Duration;

// ===== Construction =====

#[test]
fn test_new_clock_not_synchronized() {
    let clock = PtpClock::new(0x0012_3456, PtpRole::Slave);
    assert!(!clock.is_synchronized());
    assert_eq!(clock.offset_nanos(), 0);
    assert!(clock.drift_ppm().abs() < f64::EPSILON);
    assert_eq!(clock.measurement_count(), 0);
}

#[test]
fn test_clock_id() {
    let clock = PtpClock::new(0xDEAD_BEEF, PtpRole::Master);
    assert_eq!(clock.clock_id(), 0xDEAD_BEEF);
}

#[test]
fn test_clock_role() {
    let slave = PtpClock::new(0, PtpRole::Slave);
    assert_eq!(slave.role(), PtpRole::Slave);

    let master = PtpClock::new(0, PtpRole::Master);
    assert_eq!(master.role(), PtpRole::Master);
}

// ===== TimingMeasurement =====

#[test]
fn test_measurement_zero_offset_symmetric() {
    // Perfectly symmetric path, no offset.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000); // +1ms
    let t3 = PtpTimestamp::new(100, 2_000_000); // +2ms
    let t4 = PtpTimestamp::new(100, 3_000_000); // +3ms

    let m = TimingMeasurement::calculate(t1, t2, t3, t4, std::time::Instant::now());

    // offset = ((t2-t1) + (t3-t4)) / 2 = (1ms + (-1ms)) / 2 = 0
    assert_eq!(m.offset_ns, 0);
}

#[test]
fn test_measurement_positive_offset() {
    // Slave clock is 5 seconds ahead of master.
    let t1 = PtpTimestamp::new(100, 0); // master send
    let t2 = PtpTimestamp::new(105, 1_000_000); // slave recv (+5s + 1ms delay)
    let t3 = PtpTimestamp::new(105, 2_000_000); // slave send (+5s + 2ms)
    let t4 = PtpTimestamp::new(100, 3_000_000); // master recv (3ms from start)

    let m = TimingMeasurement::calculate(t1, t2, t3, t4, std::time::Instant::now());

    // offset = ((105.001 - 100.0) + (105.002 - 100.003)) / 2
    //        = (5.001 + 4.999) / 2 = 10.0 / 2 = 5.0 seconds
    let expected_ns: i128 = 5_000_000_000;
    assert!(
        (m.offset_ns - expected_ns).unsigned_abs() < 1_000,
        "Expected ~{expected_ns}, got {}",
        m.offset_ns
    );
}

#[test]
fn test_measurement_negative_offset() {
    // Slave clock is 2 seconds behind master.
    let t1 = PtpTimestamp::new(100, 0); // master send
    let t2 = PtpTimestamp::new(98, 1_000_000); // slave recv (-2s + 1ms)
    let t3 = PtpTimestamp::new(98, 2_000_000); // slave send (-2s + 2ms)
    let t4 = PtpTimestamp::new(100, 3_000_000); // master recv (3ms)

    let m = TimingMeasurement::calculate(t1, t2, t3, t4, std::time::Instant::now());

    // offset = ((98.001 - 100.0) + (98.002 - 100.003)) / 2
    //        = (-1.999 + -2.001) / 2 = -4.0 / 2 = -2.0 seconds
    let expected_ns: i128 = -2_000_000_000;
    assert!(
        (m.offset_ns - expected_ns).unsigned_abs() < 1_000,
        "Expected ~{expected_ns}, got {}",
        m.offset_ns
    );
}

#[test]
fn test_measurement_rtt_calculation() {
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 5_000_000); // +5ms
    let t3 = PtpTimestamp::new(100, 10_000_000); // +10ms
    let t4 = PtpTimestamp::new(100, 40_000_000); // +40ms

    let m = TimingMeasurement::calculate(t1, t2, t3, t4, std::time::Instant::now());

    // RTT = (t4 - t1) - (t3 - t2) = 40ms - 5ms = 35ms
    let expected_rtt = Duration::from_millis(35);
    let diff = m.rtt.abs_diff(expected_rtt);
    assert!(
        diff < Duration::from_millis(1),
        "Expected RTT ~{expected_rtt:?}, got {:?}",
        m.rtt
    );
}

#[test]
fn test_measurement_rtt_never_negative() {
    // Pathological case where t3 < t2 (should not happen in practice).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 50_000_000);
    let t3 = PtpTimestamp::new(100, 10_000_000); // t3 < t2 (unusual)
    let t4 = PtpTimestamp::new(100, 20_000_000); // t4 < t2 + t3 spread

    let m = TimingMeasurement::calculate(t1, t2, t3, t4, std::time::Instant::now());

    // RTT should be clamped to 0 (not negative).
    assert!(m.rtt >= Duration::ZERO);
}

// ===== PtpClock::process_timing =====

#[test]
fn test_process_timing_synchronizes() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);
    assert!(!clock.is_synchronized());

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    let accepted = clock.process_timing(t1, t2, t3, t4);
    assert!(accepted);
    assert!(clock.is_synchronized());
    assert_eq!(clock.measurement_count(), 1);
}

#[test]
fn test_process_timing_multiple_measurements() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);

    for i in 0..5 {
        let base = 100 + i;
        let t1 = PtpTimestamp::new(base, 0);
        let t2 = PtpTimestamp::new(base, 1_000_000);
        let t3 = PtpTimestamp::new(base, 2_000_000);
        let t4 = PtpTimestamp::new(base, 3_000_000);
        clock.process_timing(t1, t2, t3, t4);
    }

    assert_eq!(clock.measurement_count(), 5);
    assert!(clock.is_synchronized());
}

#[test]
fn test_process_timing_max_measurements_eviction() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_measurements(3);

    for i in 0..10 {
        let t1 = PtpTimestamp::new(i, 0);
        let t2 = PtpTimestamp::new(i, 1_000_000);
        let t3 = PtpTimestamp::new(i, 2_000_000);
        let t4 = PtpTimestamp::new(i, 3_000_000);
        clock.process_timing(t1, t2, t3, t4);
    }

    assert_eq!(clock.measurement_count(), 3);
}

#[test]
fn test_process_timing_offset_calculation() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);

    // Slave is 1 second ahead.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(101, 1_000_000);
    let t3 = PtpTimestamp::new(101, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);

    // offset ≈ 1 second
    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - 1000.0).abs() < 2.0,
        "Expected offset ~1000ms, got {offset_ms}ms"
    );
}

#[test]
fn test_process_timing_rejects_high_rtt() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_millis(10));

    // RTT = 500ms (way too high).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 500_000_000); // +500ms

    let accepted = clock.process_timing(t1, t2, t3, t4);
    assert!(!accepted);
    assert!(!clock.is_synchronized());
    assert_eq!(clock.measurement_count(), 0);
}

#[test]
fn test_process_timing_accepts_low_rtt() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_millis(10));

    // RTT = 2ms (acceptable).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 500_000);
    let t3 = PtpTimestamp::new(100, 1_000_000);
    let t4 = PtpTimestamp::new(100, 2_000_000);

    let accepted = clock.process_timing(t1, t2, t3, t4);
    assert!(accepted);
}

// ===== Median filter =====

#[test]
fn test_median_filter_single_measurement() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Single measurement with offset = 0.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    assert_eq!(clock.offset_nanos(), 0);
}

#[test]
fn test_median_filter_odd_count() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_measurements(5);
    clock.set_max_rtt(Duration::from_secs(10));

    // Three measurements with different offsets.
    // Offset 1: slave 1s ahead
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(101, 500_000);
    let t3 = PtpTimestamp::new(101, 1_000_000);
    let t4 = PtpTimestamp::new(100, 1_500_000);
    clock.process_timing(t1, t2, t3, t4);

    // Offset 2: slave 3s ahead (outlier)
    let t1 = PtpTimestamp::new(200, 0);
    let t2 = PtpTimestamp::new(203, 500_000);
    let t3 = PtpTimestamp::new(203, 1_000_000);
    let t4 = PtpTimestamp::new(200, 1_500_000);
    clock.process_timing(t1, t2, t3, t4);

    // Offset 3: slave 1s ahead (same as first)
    let t1 = PtpTimestamp::new(300, 0);
    let t2 = PtpTimestamp::new(301, 500_000);
    let t3 = PtpTimestamp::new(301, 1_000_000);
    let t4 = PtpTimestamp::new(300, 1_500_000);
    clock.process_timing(t1, t2, t3, t4);

    // Median should be ~1 second (outlier of 3s rejected by median).
    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - 1000.0).abs() < 5.0,
        "Expected offset ~1000ms (median filter), got {offset_ms}ms"
    );
}

// ===== Timestamp conversion =====

#[test]
fn test_remote_to_local_no_offset() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let remote = PtpTimestamp::new(100, 0);
    let local = clock.remote_to_local(remote);
    assert_eq!(local, remote);
}

#[test]
fn test_local_to_remote_no_offset() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let local = PtpTimestamp::new(100, 0);
    let remote = clock.local_to_remote(local);
    assert_eq!(remote, local);
}

#[test]
fn test_remote_to_local_with_offset() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is 5 seconds ahead.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(105, 1_000_000);
    let t3 = PtpTimestamp::new(105, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    let remote = PtpTimestamp::new(200, 0);
    let local = clock.remote_to_local(remote);

    // local should be remote - offset ≈ 200 - 5 = 195
    assert!(
        local.seconds.abs_diff(195) <= 1,
        "Expected ~195s, got {}",
        local.seconds
    );
}

#[test]
fn test_local_to_remote_with_offset() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is 5 seconds ahead.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(105, 1_000_000);
    let t3 = PtpTimestamp::new(105, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    let local = PtpTimestamp::new(195, 0);
    let remote = clock.local_to_remote(local);

    // remote should be local + offset ≈ 195 + 5 = 200
    assert!(
        remote.seconds.abs_diff(200) <= 1,
        "Expected ~200s, got {}",
        remote.seconds
    );
}

#[test]
fn test_remote_local_roundtrip() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(103, 1_000_000);
    let t3 = PtpTimestamp::new(103, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    let original = PtpTimestamp::new(500, 123_456_789);
    let remote = clock.local_to_remote(original);
    let back = clock.remote_to_local(remote);

    let diff_nanos = (back.to_nanos() - original.to_nanos()).unsigned_abs();
    assert!(
        diff_nanos < 1_000,
        "Roundtrip error too large: {diff_nanos} nanos"
    );
}

// ===== RTP to local PTP conversion =====

#[test]
fn test_rtp_to_local_ptp_basic() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let sample_rate = 44100;

    let rtp_anchor: u32 = 0;
    let ptp_anchor = PtpTimestamp::new(100, 0);

    // 44100 samples later = 1 second.
    let local_ptp = clock.rtp_to_local_ptp(44100, sample_rate, rtp_anchor, ptp_anchor);
    assert_eq!(local_ptp.seconds, 101);
    assert!(local_ptp.nanoseconds < 1_000_000); // Should be very close to 0.
}

#[test]
fn test_rtp_to_local_ptp_wrapping() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let sample_rate = 44100;

    let rtp_anchor: u32 = u32::MAX - 1000;
    let ptp_anchor = PtpTimestamp::new(100, 0);

    // Wrap around.
    let rtp_after_wrap = rtp_anchor.wrapping_add(44100);
    let local_ptp = clock.rtp_to_local_ptp(rtp_after_wrap, sample_rate, rtp_anchor, ptp_anchor);

    // Should be ~1 second after anchor.
    assert_eq!(local_ptp.seconds, 101);
}

// ===== Reset =====

#[test]
fn test_reset_clears_state() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());

    clock.reset();
    assert!(!clock.is_synchronized());
    assert_eq!(clock.measurement_count(), 0);
    assert_eq!(clock.offset_nanos(), 0);
    assert!(clock.drift_ppm().abs() < f64::EPSILON);
}

// ===== RTT accessors =====

#[test]
fn test_last_rtt_none_when_empty() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    assert!(clock.last_rtt().is_none());
}

#[test]
fn test_last_rtt_some_after_measurement() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 5_000_000);
    clock.process_timing(t1, t2, t3, t4);

    let rtt = clock.last_rtt().unwrap();
    // RTT = (5ms - 0) - (2ms - 1ms) = 5ms - 1ms = 4ms
    assert!(
        rtt.as_millis().abs_diff(4) <= 1,
        "Expected RTT ~4ms, got {rtt:?}"
    );
}

#[test]
fn test_median_rtt_none_when_empty() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    assert!(clock.median_rtt().is_none());
}

#[test]
fn test_median_rtt_with_measurements() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_secs(10));

    // Three measurements with different RTTs.
    for rtt_ms in [2, 10, 4] {
        let t1 = PtpTimestamp::new(100, 0);
        let t2 = PtpTimestamp::new(100, 500_000);
        let t3 = PtpTimestamp::new(100, 1_000_000);
        let t4 = PtpTimestamp::new(100, rtt_ms * 1_000_000); // rtt_ms ms total - 500us processing
        clock.process_timing(t1, t2, t3, t4);
    }

    let median_rtt = clock.median_rtt().unwrap();
    // Sorted RTTs approximate: 2ms-ish, 4ms-ish, 10ms-ish. Median should be ~4ms-ish.
    assert!(
        median_rtt.as_millis() > 1 && median_rtt.as_millis() < 15,
        "Median RTT out of range: {median_rtt:?}"
    );
}

// ===== Offset accessors =====

#[test]
fn test_offset_micros_conversion() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave 1 second ahead.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(101, 1_000_000);
    let t3 = PtpTimestamp::new(101, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    let offset_us = clock.offset_micros();
    // Should be ~1_000_000 microseconds.
    assert!(
        (offset_us - 1_000_000).abs() < 2_000,
        "Expected ~1000000us, got {offset_us}us"
    );
}

// ===== Configuration =====

#[test]
fn test_set_max_measurements_minimum_one() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_measurements(0);
    // Should clamp to at least 1.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert_eq!(clock.measurement_count(), 1);
}

#[test]
fn test_set_min_sync_measurements() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_min_sync_measurements(3);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);
    assert!(!clock.is_synchronized());

    clock.process_timing(t1, t2, t3, t4);
    assert!(!clock.is_synchronized());

    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());
}

// ===== Debug formatting =====

#[test]
fn test_debug_format() {
    let clock = PtpClock::new(0xABCD, PtpRole::Slave);
    let debug = format!("{clock:?}");
    assert!(debug.contains("PtpClock"));
    assert!(debug.contains("Slave"));
    assert!(debug.contains("ABCD"));
}

// ===== One-way offset estimation =====

#[test]
fn test_process_one_way_synchronizes() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);
    assert!(!clock.is_synchronized());

    // Master sends at T1=100.000, slave receives at T2=100.001 (1ms delay).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    clock.process_one_way(t1, t2);

    assert!(clock.is_synchronized());
    assert_eq!(clock.measurement_count(), 1);
    // offset = T2 - T1 = 1ms (includes one-way delay)
    assert_eq!(clock.offset_nanos(), 1_000_000);
}

#[test]
fn test_process_one_way_large_offset() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);

    // Slave is ~1 billion seconds ahead (Unix epoch vs boot-based).
    let t1 = PtpTimestamp::new(700_000, 0); // Master (boot-based)
    let t2 = PtpTimestamp::new(1_740_000_000, 0); // Slave (Unix epoch)
    clock.process_one_way(t1, t2);

    assert!(clock.is_synchronized());
    // offset ≈ 1_739_300_000 seconds
    let offset_s = clock.offset_nanos() / 1_000_000_000;
    assert_eq!(offset_s, 1_739_300_000);
}

// ===== Remote master clock ID =====

#[test]
fn test_remote_master_clock_id_initially_none() {
    let clock = PtpClock::new(0x42, PtpRole::Slave);
    assert_eq!(clock.remote_master_clock_id(), None);
}

#[test]
fn test_set_and_get_remote_master_clock_id() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);
    clock.set_remote_master_clock_id(0x50BC_9664_729E_0008);
    assert_eq!(
        clock.remote_master_clock_id(),
        Some(0x50BC_9664_729E_0008)
    );
}

#[test]
fn test_reset_clears_remote_master_clock_id() {
    let mut clock = PtpClock::new(0x42, PtpRole::Slave);
    clock.set_remote_master_clock_id(0xDEAD_BEEF);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    clock.reset();
    assert_eq!(clock.remote_master_clock_id(), None);
}

// ===== Measurements iterator =====

#[test]
fn test_measurements_iterator() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 1_000_000);
    let t3 = PtpTimestamp::new(100, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);
    clock.process_timing(t1, t2, t3, t4);

    let count = clock.measurements().count();
    assert_eq!(count, 2);
}

// ===== HomePod-like scenario: large epoch offset =====

/// Simulates the HomePod scenario: our clock uses Unix epoch (~1.74 billion seconds),
/// HomePod uses boot-based PTP time (~700,000 seconds). Tests that the offset
/// calculation and domain conversion are correct with this very large offset.
#[test]
fn test_homepod_epoch_offset_full_exchange() {
    let mut clock = PtpClock::new(0xAAAA, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_secs(1));

    // HomePod (master) sends Sync at boot-based T1 = 705,000.001s
    let t1 = PtpTimestamp::new(705_000, 1_000_000);
    // We (slave) receive at Unix-based T2 = 1,740,000,000.002s
    let t2 = PtpTimestamp::new(1_740_000_000, 2_000_000);
    // We send Delay_Req at Unix-based T3 = 1,740,000,000.003s
    let t3 = PtpTimestamp::new(1_740_000_000, 3_000_000);
    // HomePod receives at boot-based T4 = 705,000.004s
    let t4 = PtpTimestamp::new(705_000, 4_000_000);

    let accepted = clock.process_timing(t1, t2, t3, t4);
    assert!(accepted, "Measurement should be accepted (RTT is ~2ms)");
    assert!(clock.is_synchronized());

    // Expected offset = ((T2-T1) + (T3-T4)) / 2
    //   T2-T1 = 1,740,000,000.002 - 705,000.001 = 1,739,295,000.001
    //   T3-T4 = 1,740,000,000.003 - 705,000.004 = 1,739,294,999.999
    //   sum   = 3,478,590,000.000
    //   offset= 1,739,295,000.000
    let expected_offset_s: i128 = 1_739_295_000;
    let actual_offset_s = clock.offset_nanos() / 1_000_000_000;
    assert_eq!(
        actual_offset_s, expected_offset_s,
        "Offset should be ~1.739 billion seconds (Unix - boot epoch difference)"
    );

    // Now verify domain conversion: converting our Unix time to HomePod's domain.
    // remote_to_local(our_time) = our_time - offset = 1,740,000,000.5 - 1,739,295,000 = 705,000.5
    let our_time = PtpTimestamp::new(1_740_000_000, 500_000_000);
    let master_time = clock.remote_to_local(our_time);
    assert!(
        master_time.seconds.abs_diff(705_000) <= 1,
        "Expected ~705,000s in master domain, got {}s",
        master_time.seconds
    );

    // And reverse: HomePod time to our domain
    // local_to_remote(homepod_time) = 706,000 + 1,739,295,000 = 1,740,001,000
    let homepod_time = PtpTimestamp::new(706_000, 0);
    let our_equivalent = clock.local_to_remote(homepod_time);
    assert!(
        our_equivalent.seconds.abs_diff(1_740_001_000) <= 1,
        "Expected ~1,740,001,000s in our domain, got {}s",
        our_equivalent.seconds
    );
}

/// Same scenario but with one-way estimation (no Delay_Req response).
#[test]
fn test_homepod_epoch_offset_one_way() {
    let mut clock = PtpClock::new(0xAAAA, PtpRole::Slave);

    // HomePod sends at 705,000.000s (boot-based), we receive at 1,740,000,000.001s (Unix).
    // One-way delay is ~1ms, which we can't correct for without Delay_Req/Resp.
    let t1 = PtpTimestamp::new(705_000, 0);
    let t2 = PtpTimestamp::new(1_740_000_000, 1_000_000);
    clock.process_one_way(t1, t2);

    assert!(clock.is_synchronized());

    // One-way offset = T2 - T1 = 1,740,000,000.001 - 705,000.000 = 1,739,295,000.001s
    let offset_s = clock.offset_nanos() / 1_000_000_000;
    assert_eq!(offset_s, 1_739_295_000);

    // Domain conversion
    let our_time = PtpTimestamp::new(1_740_000_000, 500_000_000);
    let master_time = clock.remote_to_local(our_time);
    // master_time = our_time - offset ≈ 705,000.499s
    assert!(
        master_time.seconds.abs_diff(705_000) <= 1,
        "Expected ~705,000s, got {}s",
        master_time.seconds
    );
}

/// Verify that remote_to_local and local_to_remote form a consistent pair
/// under the HomePod scenario.
#[test]
fn test_conversion_roundtrip_large_offset() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Set up a large offset (Unix vs boot-based)
    let t1 = PtpTimestamp::new(705_000, 0);
    let t2 = PtpTimestamp::new(1_740_000_000, 1_000_000);
    let t3 = PtpTimestamp::new(1_740_000_000, 2_000_000);
    let t4 = PtpTimestamp::new(705_000, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    // Roundtrip: remote_to_local(local_to_remote(x)) == x
    let original = PtpTimestamp::new(800_000, 123_456_789);
    let converted = clock.local_to_remote(original);
    let back = clock.remote_to_local(converted);
    let error_nanos = (back.to_nanos() - original.to_nanos()).unsigned_abs();
    assert!(
        error_nanos < 1_000,
        "Roundtrip error too large: {error_nanos} nanos"
    );

    // And the other direction
    let original2 = PtpTimestamp::new(1_740_500_000, 987_654_321);
    let converted2 = clock.remote_to_local(original2);
    let back2 = clock.local_to_remote(converted2);
    let error_nanos2 = (back2.to_nanos() - original2.to_nanos()).unsigned_abs();
    assert!(
        error_nanos2 < 1_000,
        "Roundtrip error (other direction) too large: {error_nanos2} nanos"
    );
}

/// Test TimeAnnounce conversion: verify that when we compute the PTP timestamp
/// for TimeAnnounce, it correctly maps to the master's PTP domain.
#[test]
fn test_time_announce_conversion_slave_to_master_domain() {
    let mut clock = PtpClock::new(0xAAAA, PtpRole::Slave);

    // Set up HomePod scenario offset
    let t1 = PtpTimestamp::new(705_000, 0);
    let t2 = PtpTimestamp::new(1_740_000_000, 1_000_000);
    let t3 = PtpTimestamp::new(1_740_000_000, 2_000_000);
    let t4 = PtpTimestamp::new(705_000, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);

    // Set remote master's clock ID (as BMCA would)
    let homepod_clock_id = 0x50BC_9664_729E_0008_u64;
    clock.set_remote_master_clock_id(homepod_clock_id);

    // Simulate what send_time_announce does:
    let local_now = PtpTimestamp::new(1_740_000_005, 0); // Our current Unix time
    let master_time = clock.remote_to_local(local_now);
    let ptp_nanos = u64::try_from(master_time.to_nanos()).unwrap_or(0);
    let used_clock_id = clock
        .remote_master_clock_id()
        .unwrap_or_else(|| clock.clock_id());

    // master_time = local_now - offset = 1,740,000,005 - 1,739,295,000 = 705,005s
    assert!(
        master_time.seconds.abs_diff(705_005) <= 1,
        "TimeAnnounce PTP time should be ~705,005s (master domain), got {}s",
        master_time.seconds
    );

    // Verify clock ID is the HomePod's, not ours
    assert_eq!(used_clock_id, homepod_clock_id);

    // Verify the nanos value is reasonable
    let expected_nanos = 705_005u64 * 1_000_000_000;
    assert!(
        ptp_nanos.abs_diff(expected_nanos) < 2_000_000_000, // within 2s
        "PTP nanos should be ~{expected_nanos}, got {ptp_nanos}"
    );
}

/// When acting as master (no remote master), TimeAnnounce should use our own
/// clock ID and current time directly.
#[test]
fn test_time_announce_conversion_as_master() {
    let clock = PtpClock::new(0xAAAA, PtpRole::Master);

    // No offset, no remote master - we ARE the master.
    let local_now = PtpTimestamp::new(1_740_000_000, 0);
    let master_time = clock.remote_to_local(local_now);

    // With zero offset, remote_to_local should return the same time.
    assert_eq!(master_time.seconds, 1_740_000_000);

    // Clock ID should be our own (no remote master set)
    let used_clock_id = clock
        .remote_master_clock_id()
        .unwrap_or_else(|| clock.clock_id());
    assert_eq!(used_clock_id, 0xAAAA);
}

// ===== Multiple one-way measurements converge =====

#[test]
fn test_one_way_measurements_median_filter() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_measurements(5);

    // Simulate multiple one-way syncs with slight jitter.
    // Base offset: ~1,739,300,000s
    let offsets_ms = [0, 2, -1, 3, 1]; // jitter around 0ms added to base
    for jitter_ms in offsets_ms {
        let t1 = PtpTimestamp::new(705_000, 0);
        let t2_nanos = 1_000_000u32.wrapping_add((1 + jitter_ms) as u32 * 1_000_000);
        let t2 = PtpTimestamp::new(1_740_000_000, t2_nanos);
        clock.process_one_way(t1, t2);
    }

    assert_eq!(clock.measurement_count(), 5);
    assert!(clock.is_synchronized());

    // Median should filter out outliers.
    let offset_s = clock.offset_nanos() / 1_000_000_000;
    assert_eq!(
        offset_s, 1_739_295_000,
        "Median-filtered offset should be stable at ~1,739,295,000s"
    );
}

// ===== Verify offset sign convention =====

/// When slave is BEHIND master (negative offset), verify conversions are correct.
#[test]
fn test_negative_offset_conversion() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is 10 seconds BEHIND master.
    let t1 = PtpTimestamp::new(200, 0); // master send
    let t2 = PtpTimestamp::new(190, 1_000_000); // slave receive (10s behind + 1ms delay)
    let t3 = PtpTimestamp::new(190, 2_000_000); // slave send
    let t4 = PtpTimestamp::new(200, 3_000_000); // master receive

    clock.process_timing(t1, t2, t3, t4);

    // offset = ((190.001 - 200.0) + (190.002 - 200.003)) / 2
    //        = (-9.999 + -10.001) / 2 = -10.0
    let offset_s = clock.offset_nanos() / 1_000_000_000;
    assert_eq!(offset_s, -10, "Offset should be -10s (slave behind master)");

    // remote_to_local: convert master time 300s to slave equivalent.
    // slave = master + offset(?) No: remote_to_local(x) = x - offset = 300 - (-10) = 310
    let master_ts = PtpTimestamp::new(300, 0);
    let result = clock.remote_to_local(master_ts);
    assert_eq!(
        result.seconds, 310,
        "remote_to_local should ADD 10s when offset is -10s"
    );

    // local_to_remote: convert slave time 310s to master equivalent.
    // local_to_remote(x) = x + offset = 310 + (-10) = 300
    let slave_ts = PtpTimestamp::new(310, 0);
    let result = clock.local_to_remote(slave_ts);
    assert_eq!(
        result.seconds, 300,
        "local_to_remote should SUBTRACT 10s when offset is -10s"
    );
}
