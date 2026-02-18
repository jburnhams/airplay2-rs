use std::time::Duration;

use crate::audio::clock::*;

#[test]
fn test_clock_advance() {
    let clock = AudioClock::new(44100);

    clock.advance(44100);
    assert_eq!(clock.position(), 44100);

    let duration = clock.time_position();
    assert!((duration.as_secs_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_frame_duration_conversion() {
    let clock = AudioClock::new(48000);

    let frames = clock.duration_to_frames(Duration::from_secs(2));
    assert_eq!(frames, 96000);

    let duration = clock.frames_to_duration(48000);
    assert_eq!(duration.as_secs(), 1);
}
