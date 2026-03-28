use std::time::Duration;

use crate::common::audio_verify::*;

mod common;

#[test]
fn test_audio_verify_empty_buffer() {
    let audio = RawAudio {
        data: Vec::new(),
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    assert!(audio.is_empty());
    assert_eq!(audio.num_frames(), 0);
    assert_eq!(audio.duration(), Duration::from_secs(0));

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio);
    assert!(result.is_err());
    match result.unwrap_err() {
        AudioVerifyError::VerificationFailed(reason) => {
            assert_eq!(reason, "Audio is empty");
        }
        _ => panic!("Expected VerificationFailed"),
    }
}

#[test]
fn test_audio_verify_short_after_latency_skip() {
    let audio = RawAudio {
        data: vec![0; 4], // 1 frame
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio);
    assert!(result.is_err());
    match result.unwrap_err() {
        AudioVerifyError::VerificationFailed(reason) => {
            assert!(reason.contains("too short after skipping setup latency"));
        }
        _ => panic!("Expected VerificationFailed"),
    }
}

#[test]
fn test_audio_verify_entirely_silence() {
    // Generate 1s of silence
    let sample_rate = 44100;
    let data = vec![0; sample_rate * 2 * 2]; // 1s, stereo, 16-bit

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio);
    assert!(result.is_err());
    match result.unwrap_err() {
        AudioVerifyError::VerificationFailed(reason) => {
            assert!(reason.contains("entirely silence after setup latency"));
        }
        _ => panic!("Expected VerificationFailed"),
    }
}

#[test]
fn test_raw_audio_big_endian() {
    let num_samples = 44100;
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    for i in 0..num_samples {
        let sample = (32767.0 * (i as f32 / 44100.0)) as i16;
        let bytes = sample.to_be_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Big,
        signed: true,
    };

    let samples_f32 = audio.samples_f32();
    assert_eq!(samples_f32.len(), 44100 * 2);
    // Spot check a few samples to make sure BE is correctly parsed
    assert!((samples_f32[100] * 32768.0 - (32767.0 * (50.0 / 44100.0))).abs() < 1.0);
}

#[test]
fn test_raw_audio_big_endian_24bit() {
    let num_samples = 44100;
    let mut data = Vec::with_capacity(num_samples * 2 * 3);

    for i in 0..num_samples {
        let sample = (8388607.0 * (i as f32 / 44100.0)) as i32;
        let bytes = sample.to_be_bytes();
        // 24-bit is 3 bytes
        data.extend_from_slice(&bytes[1..4]);
        data.extend_from_slice(&bytes[1..4]);
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 24,
        endianness: Endianness::Big,
        signed: true,
    };

    let samples_f32 = audio.samples_f32();
    assert_eq!(samples_f32.len(), 44100 * 2);
}

#[test]
fn test_measure_gap_latency_open_gap_end() {
    let sample_rate = 44100;
    let num_samples = 44100; // 1s
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    // 0.5s of audio
    for i in 0..sample_rate / 2 {
        let sample = (32767.0 * (i as f32 / 44100.0)) as i16;
        let bytes = sample.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);
    }

    // 0.5s of silence
    for _ in 0..sample_rate / 2 {
        data.extend_from_slice(&[0, 0, 0, 0]);
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let gaps = measure_gap_latency(&audio, 40.0);
    assert_eq!(gaps.len(), 1);
    assert!(gaps[0].duration.as_millis() >= 490);
    assert!(gaps[0].duration.as_millis() <= 510);
    assert!(gaps[0].position.as_millis() >= 490);
    assert!(gaps[0].position.as_millis() <= 510);
}

#[test]
fn test_audio_verify_noise() {
    let num_samples = 44100; // 1s
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    // Generate random noise
    for _ in 0..num_samples {
        let sample_l = rand::random::<i16>();
        let sample_r = rand::random::<i16>();
        data.extend_from_slice(&sample_l.to_le_bytes());
        data.extend_from_slice(&sample_r.to_le_bytes());
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio);
    assert!(result.is_ok()); // Will parse but fail
    let res = result.unwrap();
    assert!(!res.passed);
    assert!(
        res.failure_reasons
            .iter()
            .any(|r| r.contains("Frequency error"))
    );
}

#[test]
fn test_align_audio_empty() {
    let reference = Vec::new();
    let captured = Vec::new();
    let (offset, corr) = align_audio(&reference, &captured, 10);
    assert_eq!(offset, 0);
    assert_eq!(corr, 0.0);
}
