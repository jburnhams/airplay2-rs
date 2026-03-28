use std::f32::consts::PI;
use std::time::Duration;

use crate::common::audio_verify::*;

fn generate_sine_wave(
    frequency: f32,
    sample_rate: u32,
    duration_secs: f32,
    amplitude: f32,
    channels: u16,
    bits: u16,
    endianness: Endianness,
) -> RawAudio {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut data = Vec::with_capacity(num_samples * channels as usize * (bits as usize / 8));

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample_f32 = (2.0 * PI * frequency * t).sin() * amplitude;

        for _ in 0..channels {
            if bits == 16 {
                let sample_i16 = (sample_f32 * 32767.0) as i16;
                let bytes = if endianness == Endianness::Little {
                    sample_i16.to_le_bytes()
                } else {
                    sample_i16.to_be_bytes()
                };
                data.extend_from_slice(&bytes);
            } else if bits == 24 {
                let sample_i32 = (sample_f32 * 8388607.0) as i32;
                let bytes = if endianness == Endianness::Little {
                    sample_i32.to_le_bytes()
                } else {
                    sample_i32.to_be_bytes()
                };
                if endianness == Endianness::Little {
                    data.extend_from_slice(&bytes[0..3]);
                } else {
                    data.extend_from_slice(&bytes[1..4]);
                }
            }
        }
    }

    RawAudio {
        data,
        sample_rate,
        channels,
        bits_per_sample: bits,
        endianness,
        signed: true,
    }
}

#[test]
fn test_bit_exact_pcm() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    let comp = compare_audio_exact(&audio, &audio);
    assert!(comp.sample_count_match);
    assert!(comp.bit_exact);
    assert_eq!(comp.max_sample_diff, 0);
}

#[test]
fn test_align_audio_with_offset() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);
    let original = audio.samples_f32();

    // Create a delayed version
    let offset_samples = 200;
    let mut delayed = vec![0.0; original.len() + offset_samples];
    delayed[offset_samples..].copy_from_slice(&original);

    let (align_offset, corr) = align_audio(&original, &delayed, 500);
    assert_eq!(align_offset, offset_samples);
    assert!(corr > 0.99); // highly correlated
}

#[test]
fn test_lossy_aac_snr() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    // Simulate lossy encoding by adding a tiny bit of noise
    let mut noisy_audio = audio.clone();
    for i in 0..noisy_audio.data.len() / 2 {
        let val = i16::from_le_bytes([noisy_audio.data[i * 2], noisy_audio.data[i * 2 + 1]]);
        // Add minimal noise
        let noisy_val = val.saturating_add(5);
        let bytes = noisy_val.to_le_bytes();
        noisy_audio.data[i * 2] = bytes[0];
        noisy_audio.data[i * 2 + 1] = bytes[1];
    }

    let snr = compute_snr(&audio, &noisy_audio);
    // Even with slight noise, a clean sine wave should have decent SNR > 40dB
    assert!(snr > 40.0, "SNR too low: {}", snr);
}

#[test]
fn test_diagnostic_report_format() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    let mut check = SineWaveCheck::new(440.0);
    check.frequency_tolerance_pct = 5.0;
    let result = check.verify(&audio).unwrap();

    let codec_res = verify_codec_integrity(&audio, CodecType::Pcm, Some(&audio));

    let report = audio_diagnostic_report(
        &audio,
        "received_audio_44100_2ch.raw",
        &[Box::new(result)],
        Some(&codec_res),
    );

    assert!(report.contains("Audio Diagnostic Report"));
    assert!(report.contains("Format: 16-bit LE stereo @ 44100 Hz"));
    assert!(report.contains("Duration: 1.00s (44100 frames)"));
    assert!(report.contains("Codec: Pcm"));
    assert!(report.contains("Bit-exact match: yes"));
}

#[test]
fn test_bit_exact_alac() {
    // We mock ALAC encoding/decoding as being bit-exact same as PCM
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    let codec_res = verify_codec_integrity(&audio, CodecType::Alac, Some(&audio));
    assert_eq!(codec_res.codec, CodecType::Alac);
    assert_eq!(codec_res.bit_exact, Some(true));
    assert!(codec_res.frame_count_correct);
    assert!(codec_res.issues.is_empty());
}

#[test]
fn test_sine_wave_verify_clean() {
    let audio = generate_sine_wave(440.0, 44100, 2.0, 0.8, 2, 16, Endianness::Little);

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio).unwrap();

    assert!(result.passed);
    assert!((result.measured_frequency - 440.0).abs() < 1.0);
    assert!(result.amplitude_range > 20000);
    assert!(result.max_silence_run_samples <= 1);
}

#[test]
fn test_sine_wave_wrong_frequency() {
    let audio = generate_sine_wave(880.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    let check = SineWaveCheck::new(440.0);
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
    let audio = generate_sine_wave(
        440.0,
        44100,
        1.0,
        0.1, // Low amplitude
        2,
        16,
        Endianness::Little,
    );

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio).unwrap();

    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Amplitude range"))
    );
}

#[test]
fn test_sine_wave_with_silence_gap() {
    let mut audio = generate_sine_wave(440.0, 44100, 2.0, 0.8, 2, 16, Endianness::Little);

    // Inject a 500ms gap at 1s
    let gap_start = 44100 * 4; // 1s
    let gap_len = 44100 / 2 * 4; // 0.5s
    for i in 0..gap_len {
        audio.data[gap_start + i] = 0;
    }

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio).unwrap();

    assert!(!result.passed);
    assert!(
        result
            .failure_reasons
            .iter()
            .any(|r| r.contains("Max silence run"))
    );
}

#[test]
fn test_sine_wave_leading_silence() {
    let mut audio = generate_sine_wave(440.0, 44100, 2.0, 0.8, 2, 16, Endianness::Little);

    // 500ms of leading silence
    let silence_len = 44100 / 2 * 4;
    for i in 0..silence_len {
        audio.data[i] = 0;
    }

    let latency = measure_onset_latency(&audio, 0.1);
    assert!(latency.as_millis() >= 490 && latency.as_millis() <= 510);

    // The verify check skips the first 200ms by default, but our silence is 500ms,
    // so it might fail continuity or frequency depending on how much remains.
    // Let's configure the check to tolerate more silence just for testing.
    let mut check = SineWaveCheck::new(440.0);
    check.max_silence_run_ms = 600.0;
    let result = check.verify(&audio).unwrap();
    assert!(result.passed);
}

#[test]
fn test_stereo_independent_channels() {
    // Generate different frequencies for L/R
    let num_samples = 44100;
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    for i in 0..num_samples {
        let t = i as f32 / 44100.0;
        let left = (2.0 * PI * 440.0 * t).sin() * 0.8;
        let right = (2.0 * PI * 880.0 * t).sin() * 0.8;

        data.extend_from_slice(&((left * 32767.0) as i16).to_le_bytes());
        data.extend_from_slice(&((right * 32767.0) as i16).to_le_bytes());
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let stereo_check = StereoSineCheck {
        left_frequency: 440.0,
        right_frequency: 880.0,
        frequency_tolerance_pct: 5.0,
    };

    let result = stereo_check.verify(&audio).unwrap();
    assert!(result.left.passed);
    assert!(result.right.passed);
    assert!((result.left.measured_frequency - 440.0).abs() < 2.0);
    assert!((result.right.measured_frequency - 880.0).abs() < 2.0);
}

#[test]
fn test_dc_offset_removal() {
    let num_samples = 44100 * 2;
    let mut data = Vec::with_capacity(num_samples * 2 * 2);

    for i in 0..num_samples {
        let t = i as f32 / 44100.0;
        // Signal with massive DC offset
        let sample = (2.0 * PI * 440.0 * t).sin() * 0.2 + 0.6;

        let sample_i16 = (sample * 32767.0) as i16;
        data.extend_from_slice(&sample_i16.to_le_bytes());
        data.extend_from_slice(&sample_i16.to_le_bytes());
    }

    let audio = RawAudio {
        data,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    let mut check = SineWaveCheck::new(440.0);
    check.min_amplitude = 5000; // Need to lower it because our true AC amplitude is 0.2 * 32767 ~ 6500

    let result = check.verify(&audio).unwrap();
    assert!(result.passed);
    assert!((result.measured_frequency - 440.0).abs() < 1.0);
}

#[test]
fn test_very_short_audio() {
    let audio = generate_sine_wave(
        440.0,
        44100,
        0.3, // 300ms total, setup latency skip is 200ms -> leaves 100ms
        0.8,
        2,
        16,
        Endianness::Little,
    );

    let check = SineWaveCheck::new(440.0);
    let result = check.verify(&audio).unwrap();

    assert!(result.passed);
    assert!(result.duration.as_secs_f32() < 0.11);
}

#[test]
fn test_raw_audio_format_cd() {
    let audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    assert_eq!(audio.sample_rate, 44100);
    assert_eq!(audio.channels, 2);
    assert_eq!(audio.bits_per_sample, 16);
    assert_eq!(audio.num_frames(), 44100);
    assert_eq!(audio.duration(), Duration::from_secs(1));

    let f32_samples = audio.samples_f32();
    assert_eq!(f32_samples.len(), 44100 * 2);
}

#[test]
fn test_raw_audio_24bit() {
    let audio = generate_sine_wave(440.0, 48000, 0.5, 0.5, 2, 24, Endianness::Little);

    assert_eq!(audio.sample_rate, 48000);
    assert_eq!(audio.channels, 2);
    assert_eq!(audio.bits_per_sample, 24);
    assert_eq!(audio.num_frames(), 24000);
    assert_eq!(audio.duration(), Duration::from_millis(500));

    let f32_samples = audio.samples_f32();
    assert_eq!(f32_samples.len(), 24000 * 2);

    // Check amplitude roughly matches expected
    let max_val = f32_samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));
    assert!((max_val - 0.5).abs() < 0.05);
}

#[test]
fn test_gap_detection() {
    // Generate 1s of audio
    let mut audio = generate_sine_wave(440.0, 44100, 1.0, 0.8, 2, 16, Endianness::Little);

    // Inject 50ms gaps
    let sample_rate = 44100;
    let gap_samples = (sample_rate as f32 * 0.05) as usize; // 50ms gap

    // First gap around 200ms
    let gap1_start_frame = (sample_rate as f32 * 0.2) as usize;
    let gap1_byte_start = gap1_start_frame * 4;
    let gap1_byte_len = gap_samples * 4;
    for i in 0..gap1_byte_len {
        audio.data[gap1_byte_start + i] = 0;
    }

    // Second gap around 600ms
    let gap2_start_frame = (sample_rate as f32 * 0.6) as usize;
    let gap2_byte_start = gap2_start_frame * 4;
    let gap2_byte_len = gap_samples * 4;
    for i in 0..gap2_byte_len {
        audio.data[gap2_byte_start + i] = 0;
    }

    let gaps = measure_gap_latency(&audio, 40.0);
    assert_eq!(gaps.len(), 2);

    // Check duration of first gap
    assert!(gaps[0].duration.as_millis() >= 49 && gaps[0].duration.as_millis() <= 51);
    // Check duration of second gap
    assert!(gaps[1].duration.as_millis() >= 49 && gaps[1].duration.as_millis() <= 51);
}
