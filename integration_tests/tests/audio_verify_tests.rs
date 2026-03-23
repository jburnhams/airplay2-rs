mod common;
use std::f32::consts::PI;

use common::audio_verify::*;

fn generate_sine_wave(
    frequency: f32,
    sample_rate: u32,
    duration_secs: f32,
    amplitude: f32,
    format: RawAudioFormat,
) -> RawAudio {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let bytes_per_sample = (format.bits_per_sample / 8) as usize;
    let mut data = Vec::with_capacity(num_samples * format.channels as usize * bytes_per_sample);

    let mut phase = 0.0;
    for _ in 0..num_samples {
        let sample = (phase * 2.0 * PI).sin() * amplitude;
        if format.bits_per_sample == 16 {
            let val16 = (sample * 32767.0) as i16;
            let bytes = match format.endianness {
                Endianness::Little => val16.to_le_bytes(),
                Endianness::Big => val16.to_be_bytes(),
            };

            for _ in 0..format.channels {
                data.extend_from_slice(&bytes);
            }
        } else if format.bits_per_sample == 24 {
            let val32 = (sample * 8_388_607.0) as i32;
            let bytes = match format.endianness {
                Endianness::Little => val32.to_le_bytes(),
                Endianness::Big => val32.to_be_bytes(),
            };

            // Extract the correct 3 bytes for 24-bit depending on endianness
            let byte_slice = match format.endianness {
                Endianness::Little => &bytes[0..3],
                Endianness::Big => &bytes[1..4],
            };

            for _ in 0..format.channels {
                data.extend_from_slice(byte_slice);
            }
        }

        phase += frequency / sample_rate as f32;
        if phase > 1.0 {
            phase -= 1.0;
        }
    }

    RawAudio::from_bytes(data, format)
}

#[test]
fn test_raw_audio_format_cd() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 1.0, RawAudioFormat::CD_QUALITY);
    assert_eq!(audio.num_frames(), 44100);
    assert_eq!(audio.duration().as_secs(), 1);
}

#[test]
fn test_raw_audio_format_hires() {
    let audio = generate_sine_wave(440.0, 48000, 1.0, 1.0, RawAudioFormat::HIRES);
    assert_eq!(audio.num_frames(), 48000);
    assert_eq!(audio.duration().as_secs(), 1);

    // Check 24-bit logic
    let samples_f32 = audio.samples_f32();
    assert_eq!(samples_f32.len(), 48000 * 2);

    // A full scale sine wave should have peak close to 1.0
    let peak = samples_f32.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(peak > 0.99);
}

#[test]
fn test_measure_onset_latency() {
    let sample_rate = 44100;
    let silence_duration = 0.5;
    let signal_duration = 0.5;

    let mut data = vec![0; (sample_rate as f32 * silence_duration) as usize * 4];

    let signal = generate_sine_wave(
        440.0,
        sample_rate,
        signal_duration,
        1.0,
        RawAudioFormat::CD_QUALITY,
    );
    data.extend_from_slice(&signal.data);

    let audio = RawAudio::from_bytes(data, RawAudioFormat::CD_QUALITY);

    let latency = measure_onset_latency(&audio, 0.1);

    assert!((latency.as_secs_f32() - silence_duration).abs() < 0.01);
}

#[test]
fn test_measure_gap_latency() {
    let sample_rate = 44100;

    let signal1 = generate_sine_wave(440.0, sample_rate, 0.5, 1.0, RawAudioFormat::CD_QUALITY);
    let silence = vec![0; (sample_rate as f32 * 0.1) as usize * 4]; // 100ms silence
    let signal2 = generate_sine_wave(440.0, sample_rate, 0.5, 1.0, RawAudioFormat::CD_QUALITY);

    let mut data = signal1.data.clone();
    data.extend_from_slice(&silence);
    data.extend_from_slice(&signal2.data);

    let audio = RawAudio::from_bytes(data, RawAudioFormat::CD_QUALITY);

    let gaps = measure_gap_latency(&audio, 50.0); // look for gaps > 50ms

    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].duration.as_secs_f32() - 0.1).abs() < 0.01);
    assert!((gaps[0].position.as_secs_f32() - 0.5).abs() < 0.01);
}

#[test]
fn test_raw_audio_format_cd_duplicate() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 1.0, RawAudioFormat::CD_QUALITY);
    assert_eq!(audio.num_frames(), 44100);
    assert_eq!(audio.duration().as_secs(), 1);
}

#[test]
fn test_sine_wave_verify_clean() {
    let audio = generate_sine_wave(440.0, 44100, 2.0, 0.8, RawAudioFormat::CD_QUALITY);
    let check = SineWaveCheck {
        expected_frequency: 440.0,
        min_amplitude: 20000,
        ..Default::default()
    };

    let result = check.verify(&audio).expect("Verification should succeed");
    assert!(result.passed);
    assert!((result.measured_frequency - 440.0).abs() < 1.0);
}

#[test]
fn test_sine_wave_wrong_frequency() {
    let audio = generate_sine_wave(880.0, 44100, 1.0, 0.8, RawAudioFormat::CD_QUALITY);
    let check = SineWaveCheck {
        expected_frequency: 440.0,
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Frequency mismatch"))
    );
}

#[test]
fn test_sine_wave_low_amplitude() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.1, RawAudioFormat::CD_QUALITY);
    let check = SineWaveCheck {
        expected_frequency: 440.0,
        min_amplitude: 20000, // Expected full scale but got 0.1
        ..Default::default()
    };

    let result = check.verify(&audio).unwrap();
    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Amplitude range too low"))
    );
}

#[test]
fn test_bit_exact_pcm() {
    let sent = generate_sine_wave(440.0, 44100, 1.0, 1.0, RawAudioFormat::CD_QUALITY);
    let received = generate_sine_wave(440.0, 44100, 1.0, 1.0, RawAudioFormat::CD_QUALITY);

    let cmp = compare_audio_exact(&sent, &received);
    assert!(
        cmp.bit_exact,
        "Not bit exact! Max diff: {}",
        cmp.max_sample_diff
    );
    assert!(cmp.sample_count_match);
}

#[test]
fn test_lossy_aac_snr() {
    // Simulate lossy aac with some noise
    let clean = generate_sine_wave(440.0, 44100, 1.0, 1.0, RawAudioFormat::CD_QUALITY);

    // Create a received signal with slight noise
    let mut noisy_data = clean.data.clone();
    for i in (0..noisy_data.len()).step_by(2) {
        if i + 1 < noisy_data.len() {
            let mut val = i16::from_le_bytes([noisy_data[i], noisy_data[i + 1]]);
            // add tiny noise
            val = val.saturating_add((i % 10) as i16 - 5);
            let bytes = val.to_le_bytes();
            noisy_data[i] = bytes[0];
            noisy_data[i + 1] = bytes[1];
        }
    }

    let received = RawAudio::from_bytes(noisy_data, RawAudioFormat::CD_QUALITY);
    let snr = compute_snr(&clean, &received);

    // Snr should be reasonably high but not perfect match
    assert!(snr > 60.0);
    assert!(snr < 100.0);
}

#[test]
fn test_align_audio_with_offset() {
    let reference = vec![0.1f32, 0.2, 0.3, 0.4, 0.5];
    let mut captured = vec![0.0f32; 10]; // 10 samples of zeros

    // insert reference into captured at offset 3
    for (i, &val) in reference.iter().enumerate() {
        captured[i + 3] = val;
    }

    let offset = align_audio_f32(&reference, &captured, 5);
    assert_eq!(offset, 3);
}

#[test]
fn test_stereo_independent_channels() {
    let sample_rate = 44100;
    let num_samples = sample_rate;
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    let mut phase_l = 0.0;
    let mut phase_r = 0.0;

    let freq_l = 440.0;
    let freq_r = 880.0;

    for _ in 0..num_samples {
        let sample_l = (phase_l * 2.0 * PI).sin();
        let sample_r = (phase_r * 2.0 * PI).sin();

        let val_l = (sample_l * 30000.0) as i16;
        let val_r = (sample_r * 30000.0) as i16;

        data.extend_from_slice(&val_l.to_le_bytes());
        data.extend_from_slice(&val_r.to_le_bytes());

        phase_l += freq_l / sample_rate as f32;
        if phase_l > 1.0 {
            phase_l -= 1.0;
        }

        phase_r += freq_r / sample_rate as f32;
        if phase_r > 1.0 {
            phase_r -= 1.0;
        }
    }

    let audio = RawAudio::from_bytes(data, RawAudioFormat::CD_QUALITY);

    let check = StereoSineCheck {
        left_frequency: 440.0,
        right_frequency: 880.0,
        frequency_tolerance_pct: 5.0,
    };

    let result = check.verify(&audio).unwrap();

    assert!(result.left.passed);
    assert!(result.right.passed);
    assert!((result.left.measured_frequency - 440.0).abs() < 2.0);
    assert!((result.right.measured_frequency - 880.0).abs() < 2.0);
}
