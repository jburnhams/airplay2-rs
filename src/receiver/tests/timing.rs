use crate::receiver::timing::{ClockSync, NtpTimestamp};
use std::time::Duration;

#[test]
fn test_ntp_timestamp_now() {
    let ts = NtpTimestamp::now();

    // Should be after year 2020 in NTP time
    // 2020 in NTP = 3786825600 (seconds since 1900)
    assert!(ts.seconds > 3_786_825_600);
}

#[test]
fn test_ntp_timestamp_roundtrip() {
    let original = NtpTimestamp {
        seconds: 12_345_678,
        fraction: 0xABCD_EF00,
    };

    let u64_val = original.to_u64();
    let restored = NtpTimestamp::from_u64(u64_val);

    assert_eq!(original.seconds, restored.seconds);
    assert_eq!(original.fraction, restored.fraction);
}

#[test]
fn test_ntp_diff_micros() {
    let t1 = NtpTimestamp {
        seconds: 1000,
        fraction: 0,
    };
    let t2 = NtpTimestamp {
        seconds: 1001,
        fraction: 0,
    };

    let diff = t2.diff_micros(&t1);
    assert_eq!(diff, 1_000_000); // 1 second = 1,000,000 microseconds
}

#[test]
fn test_clock_sync_update() {
    let mut sync = ClockSync::new();

    // t1: Sender transmit time = 1000.0s
    let sender = NtpTimestamp {
        seconds: 1000,
        fraction: 0,
    };

    // t2: Our receive time = 1000.5s (offset +0.5s, assuming delay is negligible for first part of calc)
    // 0x8000_0000 is exactly 0.5s in NTP fraction
    let receive = NtpTimestamp {
        seconds: 1000,
        fraction: 0x8000_0000,
    };

    // t3: Our transmit time = 1000.6s
    // 0.6s * 2^32 = 2576980377.6 -> 0x9999_9999 (approx)
    let transmit = NtpTimestamp {
        seconds: 1000,
        fraction: 0x9999_9999,
    };

    sync.update(sender, receive, transmit);

    // Calculations based on src/receiver/timing.rs logic:
    // receive_diff (t2 - t1) = 0.5s = 500,000 micros
    // transmit_diff (t3 - t2) = 0.1s = 100,000 micros (our processing delay)

    // First update uses alpha = 0.5
    // offset_avg = (1.0 - 0.5) * 0.0 + 0.5 * 500,000.0 = 250,000.0
    // offset_micros = 250,000
    // delay_micros = transmit_diff = 100,000 (roughly)

    let offset = sync.offset_micros();
    let delay = sync.delay_micros();

    // 0.5s = 500,000us. Half of that is 250,000us.
    assert_eq!(offset, 250_000);

    // 0.1s = 100,000us.
    // 0x9999_9999 (0.6) - 0x8000_0000 (0.5) = 0x1999_9999
    // 0x1999_9999 / 2^32 * 1,000,000 = 0.1 * 1,000,000 = 100,000
    // Allow small margin for integer arithmetic rounding
    assert!((i64::try_from(delay).unwrap() - 100_000).abs() < 5);
}

#[test]
fn test_ntp_to_duration() {
    let ts = NtpTimestamp {
        seconds: 1,
        fraction: 0x8000_0000,
    }; // 1.5 seconds
    let dur = ts.to_duration();
    assert_eq!(dur, Duration::new(1, 500_000_000));
}
