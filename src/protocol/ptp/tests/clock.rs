use crate::protocol::ptp::clock::{PtpClock, PtpRole, TimingMeasurement};
use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::Duration;

// ===== Construction =====

#[test]
fn test_new_clock_not_synchronized() {
    let clock = PtpClock::new(0x123456, PtpRole::Slave);
    assert!(!clock.is_synchronized());
    assert_eq!(clock.offset_nanos(), 0);
    assert_eq!(clock.drift_ppm(), 0.0);
    assert_eq!(clock.measurement_count(), 0);
}

#[test]
fn test_clock_id() {
    let clock = PtpClock::new(0xDEADBEEF, PtpRole::Master);
    assert_eq!(clock.clock_id(), 0xDEADBEEF);
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
    let diff = if m.rtt > expected_rtt {
        m.rtt - expected_rtt
    } else {
        expected_rtt - m.rtt
    };
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
        (local.seconds as i64 - 195).abs() <= 1,
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
        (remote.seconds as i64 - 200).abs() <= 1,
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

    let anchor_rtp: u32 = 0;
    let anchor_ptp = PtpTimestamp::new(100, 0);

    // 44100 samples later = 1 second.
    let local_ptp = clock.rtp_to_local_ptp(44100, sample_rate, anchor_rtp, anchor_ptp);
    assert_eq!(local_ptp.seconds, 101);
    assert!(local_ptp.nanoseconds < 1_000_000); // Should be very close to 0.
}

#[test]
fn test_rtp_to_local_ptp_wrapping() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let sample_rate = 44100;

    let anchor_rtp: u32 = u32::MAX - 1000;
    let anchor_ptp = PtpTimestamp::new(100, 0);

    // Wrap around.
    let rtp_after_wrap = anchor_rtp.wrapping_add(44100);
    let local_ptp = clock.rtp_to_local_ptp(rtp_after_wrap, sample_rate, anchor_rtp, anchor_ptp);

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
    assert_eq!(clock.drift_ppm(), 0.0);
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
        (rtt.as_millis() as i64 - 4).abs() <= 1,
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
