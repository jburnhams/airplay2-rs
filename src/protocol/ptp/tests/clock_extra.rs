use crate::protocol::ptp::clock::{PtpClock, PtpRole};
use crate::protocol::ptp::timestamp::PtpTimestamp;

#[test]
fn test_rtp_to_local_ptp_large_diff_overflow_check() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let sample_rate = 44100;

    let rtp_anchor = 0;
    let ptp_anchor = PtpTimestamp::new(100, 0);

    // u32::MAX difference (4.29e9 samples)
    // 4.29e9 * 1e9 = 4.29e18 < i64::MAX (9.22e18).
    // So overflow of intermediate calculation is safe with i64.

    let rtp_timestamp = u32::MAX;
    let local = clock.rtp_to_local_ptp(rtp_timestamp, sample_rate, rtp_anchor, ptp_anchor);

    // Wrapping logic treats u32::MAX as -1 relative to 0!
    // diff = -1.
    // So time = -1/44100.
    // ptp = 100 - epsilon.
    assert_eq!(local.seconds, 99);
    assert!(local.nanoseconds > 999_000_000);
}

#[test]
fn test_rtp_to_local_ptp_half_range_boundary() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let sample_rate = 44100;

    let rtp_anchor = 0;
    let ptp_anchor = PtpTimestamp::new(100, 0);

    // Max positive range: i32::MAX
    let rtp_timestamp = i32::MAX as u32;
    let local = clock.rtp_to_local_ptp(rtp_timestamp, sample_rate, rtp_anchor, ptp_anchor);

    // diff = 2^31 - 1. Positive.
    // 2^31 / 44100 = ~48696 seconds (13.5 hours).
    assert!(local.seconds > 100 + 48000);

    // Just over boundary: i32::MAX + 1 (which is 2^31, interpreted as negative most i32)
    let rtp_timestamp = (i32::MAX as u32) + 1;
    let local = clock.rtp_to_local_ptp(rtp_timestamp, sample_rate, rtp_anchor, ptp_anchor);

    // diff = i32::MIN (negative).
    // offset = -13.5 hours.
    assert!(local.seconds < 100);
}

#[test]
fn test_drift_calculation_with_sleep() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    // Ensure we keep enough measurements
    clock.set_max_measurements(10);

    // 1. Initial measurement at T=0
    let t1 = PtpTimestamp::new(0, 0);
    // Offset = 1ms, Delay = 1ms
    // t2 = t1 + offset + delay = 0 + 1ms + 1ms = 2ms
    let t2 = PtpTimestamp::new(0, 2_000_000);
    // t3 = t2 + processing(1ms) = 3ms
    let t3 = PtpTimestamp::new(0, 3_000_000);
    // t4 = t3 - offset + delay = 3ms - 1ms + 1ms = 3ms
    let t4 = PtpTimestamp::new(0, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);

    // 2. Sleep to let wall clock advance (> 0.1s threshold in PtpClock)
    let start = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let elapsed = start.elapsed().as_secs_f64();

    // 3. Second measurement with simulated drift of 100 ppm
    // Drift = 100 ppm = 100e-6
    // Expected offset change = elapsed * 1e9 * 100e-6 = elapsed * 1e5 ns
    let drift_ppm = 100.0;
    #[allow(clippy::cast_possible_truncation)]
    let added_offset_ns = (elapsed * drift_ppm * 1_000.0) as i128;

    // Base offset was 1_000_000
    let target_offset = 1_000_000 + added_offset_ns;

    // Construct timestamps for second measurement
    // Master moves 1s (arbitrary, just needs to be consistent for offset calc)
    let t1 = PtpTimestamp::new(1, 0);

    // t2 = t1 + offset + delay
    // t3 = t2 + processing
    // t4 = t3 - offset + delay
    // delay = 1_000_000 ns
    let t2 = PtpTimestamp::from_nanos(t1.to_nanos() + target_offset + 1_000_000);
    let t3 = PtpTimestamp::from_nanos(t2.to_nanos() + 1_000_000);
    let t4_nanos = t3.to_nanos() - target_offset + 1_000_000;
    let t4 = PtpTimestamp::from_nanos(t4_nanos);

    clock.process_timing(t1, t2, t3, t4);

    let calculated_drift = clock.drift_ppm();

    // Check with tolerance
    assert!(
        (calculated_drift - 100.0).abs() < 20.0,
        "Drift: {calculated_drift}, expected 100 (elapsed: {elapsed}s)"
    );
}
