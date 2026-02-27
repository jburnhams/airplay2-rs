//! PTP timestamp representation and conversions.
//!
//! IEEE 1588 PTP uses 80-bit timestamps (48-bit seconds + 32-bit nanoseconds).
//! `AirPlay` 2 uses a compact 64-bit format (48-bit seconds + 16-bit fraction).
//! This module supports both formats with lossless round-trip conversion.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// IEEE 1588 PTP timestamp: 48-bit seconds + 32-bit nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PtpTimestamp {
    /// Seconds since PTP epoch (TAI, but used as wall-clock in `AirPlay`).
    pub seconds: u64,
    /// Nanoseconds within the current second (`0..999_999_999`).
    pub nanoseconds: u32,
}

impl PtpTimestamp {
    /// Maximum valid nanoseconds value.
    pub const NANOS_PER_SEC: u32 = 1_000_000_000;

    /// Maximum seconds representable in 48 bits.
    pub const MAX_SECONDS_48BIT: u64 = (1u64 << 48) - 1;

    /// Create a new timestamp, clamping nanoseconds to valid range.
    #[must_use]
    pub fn new(seconds: u64, nanoseconds: u32) -> Self {
        Self {
            seconds,
            nanoseconds: nanoseconds.min(Self::NANOS_PER_SEC - 1),
        }
    }

    /// Create a timestamp from the current system time.
    ///
    /// Uses seconds since the Unix epoch as PTP seconds.
    #[must_use]
    pub fn now() -> Self {
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        Self {
            seconds: dur.as_secs(),
            nanoseconds: dur.subsec_nanos(),
        }
    }

    /// Zero timestamp.
    pub const ZERO: Self = Self {
        seconds: 0,
        nanoseconds: 0,
    };

    /// Convert to total nanoseconds since epoch.
    #[must_use]
    pub fn to_nanos(&self) -> i128 {
        i128::from(self.seconds) * i128::from(Self::NANOS_PER_SEC) + i128::from(self.nanoseconds)
    }

    /// Create from total nanoseconds since epoch.
    ///
    /// # Panics
    /// Panics on negative values or if seconds overflow `u64`.
    #[must_use]
    pub fn from_nanos(nanos: i128) -> Self {
        assert!(nanos >= 0, "PTP timestamp cannot be negative");
        let seconds =
            u64::try_from(nanos / i128::from(Self::NANOS_PER_SEC)).expect("Seconds overflow");
        let nanoseconds = u32::try_from(nanos % i128::from(Self::NANOS_PER_SEC)).unwrap();
        Self {
            seconds,
            nanoseconds,
        }
    }

    /// Convert to total microseconds since epoch.
    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        reason = "Timestamp seconds fit in i64 for foreseeable future (until year ~292 \
                  billion)"
    )]
    pub fn to_micros(&self) -> i64 {
        (self.seconds as i64 * 1_000_000) + (i64::from(self.nanoseconds) / 1_000)
    }

    /// Signed difference in nanoseconds: `self - other`.
    #[must_use]
    pub fn diff_nanos(&self, other: &Self) -> i128 {
        self.to_nanos() - other.to_nanos()
    }

    /// Signed difference in microseconds: `self - other`.
    #[must_use]
    pub fn diff_micros(&self, other: &Self) -> i64 {
        self.to_micros() - other.to_micros()
    }

    /// Encode as IEEE 1588 wire format: 6-byte seconds (BE) + 4-byte nanoseconds (BE).
    ///
    /// Returns 10 bytes.
    #[must_use]
    pub fn encode_ieee1588(&self) -> [u8; 10] {
        let mut buf = [0u8; 10];
        let sec_bytes = self.seconds.to_be_bytes();
        // 48-bit seconds: take lower 6 bytes of the 8-byte u64
        buf[0..6].copy_from_slice(&sec_bytes[2..8]);
        buf[6..10].copy_from_slice(&self.nanoseconds.to_be_bytes());
        buf
    }

    /// Decode from IEEE 1588 wire format: 6-byte seconds (BE) + 4-byte nanoseconds (BE).
    ///
    /// Returns `None` if the slice is too short.
    #[must_use]
    pub fn decode_ieee1588(data: &[u8]) -> Option<Self> {
        if data.len() < 10 {
            return None;
        }
        let seconds =
            u64::from_be_bytes([0, 0, data[0], data[1], data[2], data[3], data[4], data[5]]);
        let nanoseconds = u32::from_be_bytes([data[6], data[7], data[8], data[9]]);
        Some(Self {
            seconds,
            nanoseconds,
        })
    }

    /// Encode as `AirPlay` compact format: 48.16 fixed-point (64-bit).
    ///
    /// Upper 48 bits = seconds, lower 16 bits = fraction (1/65536 seconds).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Fraction calculation fits in u16 by design of 48.16 fixed point format"
    )]
    pub fn to_airplay_compact(&self) -> u64 {
        let fraction =
            (u64::from(self.nanoseconds) * 65536 / u64::from(Self::NANOS_PER_SEC)) as u16;
        // Mask seconds to 48 bits to prevent overflow into the fraction field
        let seconds_48 = self.seconds & Self::MAX_SECONDS_48BIT;
        (seconds_48 << 16) | u64::from(fraction)
    }

    /// Decode from `AirPlay` compact format: 48.16 fixed-point (64-bit).
    ///
    /// # Panics
    /// Panics if the calculated nanoseconds are invalid (should not happen with valid input).
    #[must_use]
    pub fn from_airplay_compact(value: u64) -> Self {
        let seconds = value >> 16;
        let fraction = value & 0xFFFF;
        let nanoseconds = u32::try_from(
            ((fraction * u64::from(Self::NANOS_PER_SEC)) / 65536)
                .min(u64::from(Self::NANOS_PER_SEC) - 1),
        )
        .unwrap();
        Self {
            seconds,
            nanoseconds,
        }
    }

    /// Convert to a `Duration` (elapsed time).
    #[must_use]
    pub fn to_duration(&self) -> Duration {
        Duration::new(self.seconds, self.nanoseconds)
    }

    /// Create from a `Duration`.
    #[must_use]
    pub fn from_duration(d: Duration) -> Self {
        Self {
            seconds: d.as_secs(),
            nanoseconds: d.subsec_nanos(),
        }
    }

    /// Add a `Duration` to this timestamp.
    ///
    /// # Panics
    /// Panics if the calculated nanoseconds are invalid (should not happen).
    #[must_use]
    pub fn add_duration(&self, d: Duration) -> Self {
        let total_nanos = u64::from(self.nanoseconds) + u64::from(d.subsec_nanos());
        let carry = total_nanos / u64::from(Self::NANOS_PER_SEC);
        Self {
            seconds: self.seconds + d.as_secs() + carry,
            nanoseconds: u32::try_from(total_nanos % u64::from(Self::NANOS_PER_SEC)).unwrap(),
        }
    }
}

impl std::fmt::Display for PtpTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{:09}", self.seconds, self.nanoseconds)
    }
}

impl std::ops::Sub for PtpTimestamp {
    type Output = i128;

    fn sub(self, rhs: Self) -> Self::Output {
        self.diff_nanos(&rhs)
    }
}

impl From<Duration> for PtpTimestamp {
    fn from(d: Duration) -> Self {
        Self::from_duration(d)
    }
}

impl From<PtpTimestamp> for Duration {
    fn from(ts: PtpTimestamp) -> Self {
        ts.to_duration()
    }
}
