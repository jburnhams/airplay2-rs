use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

use crate::receiver::control_receiver::SyncPacket;
use crate::receiver::playback_timing::PlaybackTiming;
use crate::receiver::timing::{ClockSync, NtpTimestamp};

#[tokio::test]
async fn test_playback_timing() {
    let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
    let mut timing = PlaybackTiming::new(44100, clock_sync);

    // Set reference
    let sync = SyncPacket {
        extension: false,
        rtp_timestamp: 44100,
        ntp_timestamp: NtpTimestamp::now().to_u64(),
        rtp_timestamp_at_ntp: 44100,
    };
    timing.update_from_sync(&sync);

    // Timestamp one second later (44100 samples) should play ~1 second later + latency
    let playback = timing.playback_time(44100 + 44100);
    assert!(playback.is_some());
}

#[test]
fn test_rtp_to_duration() {
    let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
    let timing = PlaybackTiming::new(44100, clock_sync);

    let duration = timing.rtp_to_duration(44100);
    assert!((duration.as_secs_f64() - 1.0).abs() < 0.001);

    let duration = timing.rtp_to_duration(22050);
    assert!((duration.as_secs_f64() - 0.5).abs() < 0.001);
}

#[tokio::test]
async fn test_playback_timing_past() {
    let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
    let mut timing = PlaybackTiming::new(44100, clock_sync);

    let sync = SyncPacket {
        extension: false,
        rtp_timestamp: 44100,
        ntp_timestamp: NtpTimestamp::now().to_u64(),
        rtp_timestamp_at_ntp: 44100,
    };
    timing.update_from_sync(&sync);

    // Past timestamp (e.g. 0)
    let playback = timing.playback_time(0);
    assert!(playback.is_some());
}

#[tokio::test]
async fn test_playback_timing_negative_diff() {
    let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
    let mut timing = PlaybackTiming::new(44100, clock_sync);

    let sync = SyncPacket {
        extension: false,
        rtp_timestamp: 44100,
        ntp_timestamp: NtpTimestamp::now().to_u64(),
        rtp_timestamp_at_ntp: 44100,
    };
    timing.update_from_sync(&sync);

    // Requesting a timestamp significantly in the past (before the reference)
    // Reference is 44100. Request 22050 (0.5s before reference).
    // samples_diff should be 22050 - 44100 = -22050 (approx -0.5s).
    // Target latency is 2.0s.
    // Expected delay from now: -0.5s + 2.0s = 1.5s (minus elapsed execution time).

    let playback = timing
        .playback_time(22050)
        .expect("Should have playback time");

    let now = Instant::now();

    // If playback is in the future relative to now:
    if playback > now {
        let delay = playback - now;
        // With the bug (unsigned wrapping), delay would be massive (~27 hours).
        // Correct delay is approx 1.5s.
        assert!(
            delay.as_secs() < 10,
            "Delay is too large: {delay:?}. Likely due to unsigned wrapping of negative timestamp \
             difference.",
        );
    } else {
        // If playback is already past (e.g. debugging slow test), it's definitely not 27 hours in
        // future. This is fine.
    }
}
