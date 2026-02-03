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
