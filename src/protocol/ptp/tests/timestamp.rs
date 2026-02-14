use crate::protocol::ptp::timestamp::PtpTimestamp;
use std::time::Duration;

// ===== Construction =====

#[test]
fn test_new_clamps_nanoseconds() {
    let ts = PtpTimestamp::new(10, 2_000_000_000);
    assert_eq!(ts.seconds, 10);
    assert_eq!(ts.nanoseconds, PtpTimestamp::NANOS_PER_SEC - 1);
}

#[test]
fn test_new_valid_nanoseconds() {
    let ts = PtpTimestamp::new(42, 500_000_000);
    assert_eq!(ts.seconds, 42);
    assert_eq!(ts.nanoseconds, 500_000_000);
}

#[test]
fn test_zero_constant() {
    assert_eq!(PtpTimestamp::ZERO.seconds, 0);
    assert_eq!(PtpTimestamp::ZERO.nanoseconds, 0);
}

#[test]
fn test_now_returns_reasonable_value() {
    let ts = PtpTimestamp::now();
    // Should be after 2020-01-01 (1577836800 seconds since Unix epoch)
    assert!(ts.seconds > 1_577_836_800, "Timestamp too old: {ts}");
    assert!(ts.nanoseconds < PtpTimestamp::NANOS_PER_SEC);
}

// ===== Nanosecond conversions =====

#[test]
fn test_to_nanos_zero() {
    assert_eq!(PtpTimestamp::ZERO.to_nanos(), 0);
}

#[test]
fn test_to_nanos_one_second() {
    let ts = PtpTimestamp::new(1, 0);
    assert_eq!(ts.to_nanos(), 1_000_000_000);
}

#[test]
fn test_to_nanos_subsecond() {
    let ts = PtpTimestamp::new(0, 500_000_000);
    assert_eq!(ts.to_nanos(), 500_000_000);
}

#[test]
fn test_to_nanos_combined() {
    let ts = PtpTimestamp::new(3, 250_000_000);
    assert_eq!(ts.to_nanos(), 3_250_000_000);
}

#[test]
fn test_from_nanos_roundtrip() {
    let original = PtpTimestamp::new(1234, 567_890_123);
    let nanos = original.to_nanos();
    let back = PtpTimestamp::from_nanos(nanos);
    assert_eq!(original, back);
}

#[test]
fn test_from_nanos_zero() {
    let ts = PtpTimestamp::from_nanos(0);
    assert_eq!(ts, PtpTimestamp::ZERO);
}

#[test]
#[should_panic(expected = "cannot be negative")]
fn test_from_nanos_negative_panics() {
    let _ = PtpTimestamp::from_nanos(-1);
}

// ===== Microsecond conversions =====

#[test]
fn test_to_micros() {
    let ts = PtpTimestamp::new(1, 500_000_000);
    assert_eq!(ts.to_micros(), 1_500_000);
}

#[test]
fn test_to_micros_zero() {
    assert_eq!(PtpTimestamp::ZERO.to_micros(), 0);
}

// ===== Differences =====

#[test]
fn test_diff_nanos_positive() {
    let a = PtpTimestamp::new(10, 0);
    let b = PtpTimestamp::new(5, 0);
    assert_eq!(a.diff_nanos(&b), 5_000_000_000);
}

#[test]
fn test_diff_nanos_negative() {
    let a = PtpTimestamp::new(5, 0);
    let b = PtpTimestamp::new(10, 0);
    assert_eq!(a.diff_nanos(&b), -5_000_000_000);
}

#[test]
fn test_diff_nanos_subsecond() {
    let a = PtpTimestamp::new(1, 750_000_000);
    let b = PtpTimestamp::new(1, 250_000_000);
    assert_eq!(a.diff_nanos(&b), 500_000_000);
}

#[test]
fn test_diff_micros_positive() {
    let a = PtpTimestamp::new(10, 500_000_000);
    let b = PtpTimestamp::new(10, 0);
    assert_eq!(a.diff_micros(&b), 500_000);
}

#[test]
fn test_sub_operator() {
    let a = PtpTimestamp::new(10, 0);
    let b = PtpTimestamp::new(5, 500_000_000);
    assert_eq!(a - b, 4_500_000_000i128);
}

// ===== IEEE 1588 encoding =====

#[test]
fn test_encode_ieee1588_zero() {
    let buf = PtpTimestamp::ZERO.encode_ieee1588();
    assert_eq!(buf, [0u8; 10]);
}

#[test]
fn test_encode_ieee1588_one_second() {
    let ts = PtpTimestamp::new(1, 0);
    let buf = ts.encode_ieee1588();
    assert_eq!(&buf[0..6], &[0, 0, 0, 0, 0, 1]);
    assert_eq!(&buf[6..10], &[0, 0, 0, 0]);
}

#[test]
fn test_encode_ieee1588_with_nanos() {
    let ts = PtpTimestamp::new(0, 1_000_000); // 1ms
    let buf = ts.encode_ieee1588();
    let nanos = u32::from_be_bytes([buf[6], buf[7], buf[8], buf[9]]);
    assert_eq!(nanos, 1_000_000);
}

#[test]
fn test_decode_ieee1588_roundtrip() {
    let original = PtpTimestamp::new(12345, 987_654_321);
    let encoded = original.encode_ieee1588();
    let decoded = PtpTimestamp::decode_ieee1588(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_decode_ieee1588_large_seconds() {
    let original = PtpTimestamp::new(0x0000_FFFF_FFFF_FFFF, 0);
    let encoded = original.encode_ieee1588();
    let decoded = PtpTimestamp::decode_ieee1588(&encoded).unwrap();
    assert_eq!(decoded.seconds, PtpTimestamp::MAX_SECONDS_48BIT);
}

#[test]
fn test_decode_ieee1588_too_short() {
    let buf = [0u8; 9];
    assert!(PtpTimestamp::decode_ieee1588(&buf).is_none());
}

#[test]
fn test_decode_ieee1588_exact_length() {
    let buf = [0u8; 10];
    assert!(PtpTimestamp::decode_ieee1588(&buf).is_some());
}

// ===== AirPlay compact format =====

#[test]
fn test_airplay_compact_zero() {
    let ts = PtpTimestamp::ZERO;
    assert_eq!(ts.to_airplay_compact(), 0);
}

#[test]
fn test_airplay_compact_one_second() {
    let ts = PtpTimestamp::new(1, 0);
    let compact = ts.to_airplay_compact();
    assert_eq!(compact, 0x0001_0000);
}

#[test]
fn test_airplay_compact_half_second() {
    let ts = PtpTimestamp::new(0, 500_000_000);
    let compact = ts.to_airplay_compact();
    // Half second in 16-bit fraction ≈ 0x8000
    assert_eq!(compact, 0x8000);
}

#[test]
fn test_airplay_compact_roundtrip() {
    let original = PtpTimestamp::new(100, 0);
    let compact = original.to_airplay_compact();
    let back = PtpTimestamp::from_airplay_compact(compact);
    assert_eq!(back.seconds, original.seconds);
    // Nanoseconds may lose precision due to 16-bit fraction.
}

#[test]
fn test_airplay_compact_roundtrip_precision() {
    // 1 second should be exact.
    let original = PtpTimestamp::new(1, 0);
    let back = PtpTimestamp::from_airplay_compact(original.to_airplay_compact());
    assert_eq!(original, back);
}

#[test]
fn test_airplay_compact_from_known_value() {
    // 0x00010000 = 1 second in AirPlay format.
    let ts = PtpTimestamp::from_airplay_compact(0x0001_0000);
    assert_eq!(ts.seconds, 1);
    assert_eq!(ts.nanoseconds, 0);
}

#[test]
fn test_airplay_compact_precision_loss_bounded() {
    // 16-bit fraction gives ~15.26 microsecond resolution.
    // Any original nanosecond value should round-trip within ~30us.
    for nanos in [0, 1000, 100_000, 500_000_000, 999_000_000] {
        let original = PtpTimestamp::new(100, nanos);
        let back = PtpTimestamp::from_airplay_compact(original.to_airplay_compact());
        let diff = (i64::from(back.nanoseconds) - i64::from(nanos)).unsigned_abs();
        assert!(
            diff < 30_000,
            "Precision loss too large: original={nanos} back={} diff={diff}",
            back.nanoseconds
        );
    }
}

// ===== Duration conversions =====

#[test]
fn test_to_duration() {
    let ts = PtpTimestamp::new(5, 500_000_000);
    let d = ts.to_duration();
    assert_eq!(d, Duration::new(5, 500_000_000));
}

#[test]
fn test_from_duration() {
    let d = Duration::new(3, 750_000_000);
    let ts = PtpTimestamp::from_duration(d);
    assert_eq!(ts.seconds, 3);
    assert_eq!(ts.nanoseconds, 750_000_000);
}

#[test]
fn test_duration_roundtrip() {
    let original = Duration::new(123, 456_789_012);
    let ts = PtpTimestamp::from_duration(original);
    let back = ts.to_duration();
    assert_eq!(original, back);
}

#[test]
fn test_from_into_duration_traits() {
    let d = Duration::new(10, 123);
    let ts: PtpTimestamp = d.into();
    assert_eq!(ts.seconds, 10);
    assert_eq!(ts.nanoseconds, 123);

    let back: Duration = ts.into();
    assert_eq!(back, d);
}

// ===== add_duration =====

#[test]
fn test_add_duration_no_carry() {
    let ts = PtpTimestamp::new(10, 100_000_000);
    let result = ts.add_duration(Duration::from_millis(500));
    assert_eq!(result.seconds, 10);
    assert_eq!(result.nanoseconds, 600_000_000);
}

#[test]
fn test_add_duration_with_carry() {
    let ts = PtpTimestamp::new(10, 800_000_000);
    let result = ts.add_duration(Duration::from_millis(500));
    assert_eq!(result.seconds, 11);
    assert_eq!(result.nanoseconds, 300_000_000);
}

#[test]
fn test_add_duration_zero() {
    let ts = PtpTimestamp::new(5, 123);
    let result = ts.add_duration(Duration::ZERO);
    assert_eq!(result, ts);
}

// ===== Display =====

#[test]
fn test_display() {
    let ts = PtpTimestamp::new(42, 123_456_789);
    assert_eq!(format!("{ts}"), "42.123456789");
}

#[test]
fn test_display_zero() {
    assert_eq!(format!("{}", PtpTimestamp::ZERO), "0.000000000");
}

// ===== Ordering =====

#[test]
fn test_ordering() {
    let a = PtpTimestamp::new(10, 0);
    let b = PtpTimestamp::new(10, 1);
    let c = PtpTimestamp::new(11, 0);
    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
}

#[test]
fn test_equality() {
    let a = PtpTimestamp::new(10, 500);
    let b = PtpTimestamp::new(10, 500);
    assert_eq!(a, b);
}
