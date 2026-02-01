use crate::receiver::control_receiver::SyncPacket;
use crate::receiver::playback_timing::PlaybackTiming;
use crate::receiver::timing::{ClockSync, NtpTimestamp};
use std::sync::Arc;
use tokio::sync::RwLock;

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
    // samples_diff will be 22050 - 44100 = -22050.
    // This previously panicked in Duration::from_secs_f64 with negative value.
    let playback = timing.playback_time(22050);
    assert!(playback.is_some());
}
