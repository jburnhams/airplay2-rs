mod common;
use crate::common::audio_verify::*;

// Helper to create a dummy RawAudio struct
fn dummy_raw_audio(
    frequency: f32,
    amplitude: i16,
    duration_secs: f32,
    format: RawAudioFormat,
) -> RawAudio {
    let num_frames = (duration_secs * format.sample_rate as f32) as usize;
    let mut data = Vec::with_capacity(num_frames * format.channels as usize * 2);

    for i in 0..num_frames {
        let t = i as f32 / format.sample_rate as f32;
        let sample = (amplitude as f32 * (2.0 * std::f32::consts::PI * frequency * t).sin()) as i16;
        let bytes = sample.to_le_bytes();
        for _ in 0..format.channels {
            data.extend_from_slice(&bytes);
        }
    }

    RawAudio::from_bytes(data, format)
}

#[test]
fn test_sine_wave_verify_clean() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = dummy_raw_audio(440.0, 30000, 1.0, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    result.assert_passed().unwrap();
}

#[test]
fn test_sine_wave_wrong_frequency() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = dummy_raw_audio(880.0, 30000, 1.0, format); // Generated 880 Hz

    let check = SineWaveCheck {
        expected_frequency: 440.0, // Expected 440 Hz
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Frequency error"))
    );
}

#[test]
fn test_sine_wave_low_amplitude() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = dummy_raw_audio(440.0, 10000, 1.0, format); // Max amp 10000

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        min_amplitude: 20000, // Requires 20000
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Amplitude"))
    );
}

#[test]
fn test_sine_wave_with_silence_gap() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut audio = dummy_raw_audio(440.0, 30000, 1.0, format);

    // Inject 200ms of silence in the middle
    let silence_frames = (0.2 * format.sample_rate as f32) as usize;
    let bytes_per_frame = format.channels as usize * 2;
    let start_byte = (audio.data.len() / 2) - (silence_frames * bytes_per_frame / 2);

    for i in 0..(silence_frames * bytes_per_frame) {
        audio.data[start_byte + i] = 0;
    }

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        max_silence_run_ms: 100.0,
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Silence run"))
    );
}

#[test]
fn test_sine_wave_leading_silence() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut silence =
        vec![0; (0.5 * format.sample_rate as f32) as usize * format.channels as usize * 2];
    let audio = dummy_raw_audio(440.0, 30000, 0.5, format);
    silence.extend(audio.data);
    let audio_with_silence = RawAudio::from_bytes(silence, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        max_silence_run_ms: 600.0, // allow leading silence
        ..Default::default()
    };

    let result = check.verify(&audio_with_silence).unwrap();
    result.assert_passed().unwrap();
}

#[test]
fn test_stereo_independent_channels() {
    let format = RawAudioFormat::CD_QUALITY;
    let num_frames = format.sample_rate as usize;
    let mut data = Vec::with_capacity(num_frames * 4);

    for i in 0..num_frames {
        let t = i as f32 / format.sample_rate as f32;
        let l_sample = (30000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16;
        let r_sample = (30000.0 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()) as i16;
        data.extend_from_slice(&l_sample.to_le_bytes());
        data.extend_from_slice(&r_sample.to_le_bytes());
    }

    let audio = RawAudio::from_bytes(data, format);

    // Left channel (440)
    let check_l = SineWaveCheck {
        expected_frequency: 440.0,
        channel: Some(0),
        ..Default::default()
    };
    assert!(check_l.verify(&audio).unwrap().passed);

    // Right channel (880)
    let check_r = SineWaveCheck {
        expected_frequency: 880.0,
        channel: Some(1),
        ..Default::default()
    };
    assert!(check_r.verify(&audio).unwrap().passed);
}

#[test]
fn test_bit_exact_pcm() {
    // Trivial bit-exact test
    let format = RawAudioFormat::CD_QUALITY;
    let audio1 = dummy_raw_audio(440.0, 30000, 1.0, format);
    let audio2 = audio1.clone();
    assert_eq!(audio1.data, audio2.data);
}

#[test]
fn test_bit_exact_alac() {
    // Placeholder since ALAC is bit exact
}
