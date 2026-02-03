use crate::audio::concealment::{Concealer, ConcealmentStrategy};

#[test]
fn test_silence_concealment() {
    let concealer = Concealer::new(ConcealmentStrategy::Silence, 44100, 4);
    let output = concealer.conceal(352);

    assert_eq!(output.len(), 352 * 4);
    assert!(output.iter().all(|&b| b == 0));
}

#[test]
fn test_repeat_concealment() {
    let mut concealer = Concealer::new(ConcealmentStrategy::Repeat, 44100, 4);

    let audio = vec![0xAB; 1408]; // 352 samples * 4 bytes
    concealer.record_good_packet(&audio);

    let output = concealer.conceal(352);
    assert_eq!(output, audio);
}

#[test]
fn test_repeat_no_previous() {
    let concealer = Concealer::new(ConcealmentStrategy::Repeat, 44100, 4);
    let output = concealer.conceal(352);

    assert_eq!(output.len(), 352 * 4);
    assert!(output.iter().all(|&b| b == 0));
}

#[test]
fn test_fade_out_stereo() {
    // 16-bit stereo = 4 bytes per frame
    let mut concealer = Concealer::new(ConcealmentStrategy::FadeOut, 44100, 4);

    // Create a constant signal: 10 frames of (1000, 1000)
    // 1000 = 0x03E8 -> [0xE8, 0x03]
    let mut audio = Vec::new();
    for _ in 0..10 {
        audio.extend_from_slice(&[0xE8, 0x03, 0xE8, 0x03]);
    }
    concealer.record_good_packet(&audio);

    // Conceal 10 frames
    let output = concealer.conceal(10);

    assert_eq!(output.len(), 40);

    // Check fading behavior
    // First frame (i=0) should have fade = 1.0 -> 1000
    // Last frame (i=9) should have fade = 0.1 -> 100
    // Actually fade calculation is 1.0 - (i / N)
    // i=0: fade=1.0 -> 1000
    // i=9: fade=0.1 -> 100

    // Check first sample of first frame
    let s0 = i16::from_le_bytes([output[0], output[1]]);
    assert!(
        (s0 - 1000).abs() < 5,
        "First sample should be ~1000, got {s0}",
    );

    // Check first sample of last frame (index 9*4 = 36)
    let s_last = i16::from_le_bytes([output[36], output[37]]);
    // i=9, N=10, fade = 1 - 0.9 = 0.1. 1000 * 0.1 = 100
    assert!(
        (s_last - 100).abs() < 5,
        "Last sample should be ~100, got {s_last}",
    );

    // Check right channel of last frame matches left channel (index 38)
    let s_last_r = i16::from_le_bytes([output[38], output[39]]);
    assert_eq!(s_last, s_last_r, "Channels should fade identically");
}
