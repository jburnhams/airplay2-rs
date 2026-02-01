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

    let sender = NtpTimestamp {
        seconds: 1000,
        fraction: 0,
    };
    let receive = NtpTimestamp {
        seconds: 1000,
        fraction: 0x8000_0000,
    }; // +0.5s
    let transmit = NtpTimestamp {
        seconds: 1000,
        fraction: 0x8000_0001,
    };

    sync.update(sender, receive, transmit);

    // Initial update might not set everything fully stable but exchange count should be 1
    // Accessing private fields via public methods (if available) or debug formatted string check
    // Since we don't have public accessors for exchange_count, we check via public methods
    assert!(sync.offset_micros() != 0 || sync.delay_micros() != 0);
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
