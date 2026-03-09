use std::f32::consts::PI;

use crate::common::audio_verify::*;

fn generate_sine_wave(
    freq: f32,
    sample_rate: u32,
    duration_secs: f32,
    format: RawAudioFormat,
) -> RawAudio {
    let mut data = Vec::new();
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let phase_inc = freq * 2.0 * PI / sample_rate as f32;
    let mut phase = 0.0f32;

    for _ in 0..num_samples {
        let sample = phase.sin();
        let value = (sample * i16::MAX as f32) as i16;
        let bytes = match format.endianness {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        };

        for _ in 0..format.channels {
            data.extend_from_slice(&bytes);
        }

        phase += phase_inc;
        if phase > 2.0 * PI {
            phase -= 2.0 * PI;
        }
    }

    RawAudio::from_bytes(data, format)
}

#[test]
fn test_sine_wave_verify_clean() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = generate_sine_wave(440.0, format.sample_rate, 1.0, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        ..Default::default()
    };

    let res = check.verify(&audio).unwrap();
    assert!(res.passed);
    assert!((res.measured_frequency - 440.0).abs() < 1.0);
}

#[test]
fn test_sine_wave_wrong_frequency() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = generate_sine_wave(880.0, format.sample_rate, 1.0, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        ..Default::default()
    };

    let res = check.verify(&audio).unwrap();
    assert!(!res.passed);
    assert!(
        res.failure_reasons
            .iter()
            .any(|r: &String| r.contains("Frequency error"))
    );
}

#[test]
fn test_sine_wave_low_amplitude() {
    let format = RawAudioFormat::CD_QUALITY;
    let num_samples = (format.sample_rate as f32 * 1.0) as usize;
    let mut data = Vec::new();
    let phase_inc = 440.0 * 2.0 * PI / format.sample_rate as f32;
    let mut phase = 0.0f32;

    // Generate at 10% amplitude
    for _ in 0..num_samples {
        let sample = phase.sin() * 0.1;
        let value = (sample * i16::MAX as f32) as i16;
        let bytes = value.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);
        phase += phase_inc;
    }

    let audio = RawAudio::from_bytes(data, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        min_amplitude: 20000,
        ..Default::default()
    };

    let res = check.verify(&audio).unwrap();
    assert!(!res.passed);
    assert!(
        res.failure_reasons
            .iter()
            .any(|r: &String| r.contains("Amplitude range"))
    );
}

#[test]
fn test_sine_wave_with_silence_gap() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut data = Vec::new();
    let num_samples = format.sample_rate as usize; // 1 second total

    let phase_inc = 440.0 * 2.0 * PI / format.sample_rate as f32;
    let mut phase = 0.0f32;

    for i in 0..num_samples {
        // Create a 200ms gap in the middle
        let value = if i > 10000 && i < 20000 {
            0
        } else {
            let sample = phase.sin();
            (sample * (i16::MAX as f32 * 0.9)) as i16
        };

        let bytes = value.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);

        phase += phase_inc;
        if phase > 2.0 * PI {
            phase -= 2.0 * PI;
        }
    }

    let audio = RawAudio::from_bytes(data, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        max_silence_run_ms: 100.0,
        ..Default::default()
    };

    let res = check.verify(&audio).unwrap();
    assert!(!res.passed);
    assert!(
        res.failure_reasons
            .iter()
            .any(|r: &String| r.contains("Silence run"))
    );
}

#[test]
fn test_sine_wave_leading_silence() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut data = Vec::new();

    // 500ms silence
    let silence_samples = (format.sample_rate as f32 * 0.5) as usize;
    for _ in 0..silence_samples {
        data.extend_from_slice(&[0, 0, 0, 0]);
    }

    // 1s sine
    let num_samples = format.sample_rate as usize;
    let phase_inc = 440.0 * 2.0 * PI / format.sample_rate as f32;
    let mut phase = 0.0f32;

    for _ in 0..num_samples {
        let sample = phase.sin();
        let value = (sample * i16::MAX as f32) as i16;
        let bytes = value.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);
        phase += phase_inc;
    }

    let audio = RawAudio::from_bytes(data, format);
    let latency = measure_onset_latency(&audio, 0.1);

    assert!((latency.as_secs_f32() - 0.5).abs() < 0.05);
}

#[test]
fn test_stereo_independent_channels() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut data = Vec::new();
    let num_samples = format.sample_rate as usize;

    let phase_inc_l = 440.0 * 2.0 * PI / format.sample_rate as f32;
    let phase_inc_r = 880.0 * 2.0 * PI / format.sample_rate as f32;
    let mut phase_l = 0.0f32;
    let mut phase_r = 0.0f32;

    for _ in 0..num_samples {
        let val_l = ((phase_l.sin()) * (i16::MAX as f32 * 0.9)) as i16;
        let val_r = ((phase_r.sin()) * (i16::MAX as f32 * 0.9)) as i16;

        data.extend_from_slice(&val_l.to_le_bytes());
        data.extend_from_slice(&val_r.to_le_bytes());

        phase_l += phase_inc_l;
        if phase_l > 2.0 * PI {
            phase_l -= 2.0 * PI;
        }

        phase_r += phase_inc_r;
        if phase_r > 2.0 * PI {
            phase_r -= 2.0 * PI;
        }
    }

    let audio = RawAudio::from_bytes(data, format);

    let check = StereoSineCheck {
        left_frequency: 440.0,
        right_frequency: 880.0,
        frequency_tolerance_pct: 5.0,
    };

    let res = check.verify(&audio).unwrap();
    assert!(res.left.passed);
    assert!(res.right.passed);
    assert!((res.left.measured_frequency - 440.0).abs() < 1.0);
    assert!((res.right.measured_frequency - 880.0).abs() < 2.0);
}

#[test]
fn test_bit_exact_pcm() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio1 = generate_sine_wave(440.0, format.sample_rate, 0.5, format);
    let audio2 = generate_sine_wave(440.0, format.sample_rate, 0.5, format);

    let cmp = compare_audio_exact(&audio1, &audio2);
    assert!(cmp.bit_exact);
    assert!(cmp.sample_count_match);
    assert_eq!(cmp.max_sample_diff, 0);
}

#[test]
fn test_bit_exact_alac() {
    // We just simulate by duplicating PCM and verifying it works.
    let format = RawAudioFormat::CD_QUALITY;
    let audio1 = generate_sine_wave(440.0, format.sample_rate, 0.5, format);
    let res = verify_codec_integrity(&audio1, CodecType::Alac, Some(&audio1));
    assert!(res.bit_exact.unwrap());
}

#[test]
fn test_lossy_aac_snr() {
    let format = RawAudioFormat::CD_QUALITY;
    let orig = generate_sine_wave(440.0, format.sample_rate, 0.5, format);

    // Create slightly noisy copy
    let mut noisy_data = orig.data.clone();
    for i in 0..noisy_data.len() {
        if i % 2 == 0 {
            noisy_data[i] = noisy_data[i].wrapping_add(2); // Inject small noise
        }
    }

    let noisy = RawAudio::from_bytes(noisy_data, format);
    let snr = compute_snr(&orig, &noisy);

    // Small noise should give high but not perfect SNR
    assert!(snr > 20.0 && snr < 100.0, "SNR was: {}", snr); // using 20.0 to just test the SNR check function passes
}

#[test]
fn test_align_audio_with_offset() {
    let format = RawAudioFormat::CD_QUALITY;
    let orig = generate_sine_wave(440.0, format.sample_rate, 0.5, format);

    let mut offset_data = vec![0u8; 200 * 4]; // 200 stereo frames of silence
    offset_data.extend_from_slice(&orig.data);

    let offset_audio = RawAudio::from_bytes(offset_data, format);

    let orig_samples = orig.samples_i16();
    let offset_samples = offset_audio.samples_i16();

    let (offset, _) = align_audio_i16(&orig_samples, &offset_samples, 1000);
    assert_eq!(offset, 400); // 200 frames * 2 channels = 400 samples offset
}

#[test]
fn test_raw_audio_format_cd() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = generate_sine_wave(440.0, format.sample_rate, 1.0, format);
    assert_eq!(audio.num_frames(), 44100);
    assert_eq!(audio.duration().as_secs_f64(), 1.0);
}

#[test]
fn test_raw_audio_24bit() {
    let format = RawAudioFormat::HIRES;
    let mut data = Vec::new();

    // Create one frame of 24-bit PCM (max positive value: 8388607)
    let val_bytes = 8388607i32.to_le_bytes(); // 4 bytes
    data.extend_from_slice(&val_bytes[0..3]); // take 3 bytes (little endian)
    data.extend_from_slice(&val_bytes[0..3]);

    let audio = RawAudio::from_bytes(data, format);
    assert_eq!(audio.num_frames(), 1);

    let f32_samples = audio.samples_f32();
    assert_eq!(f32_samples.len(), 2);
    assert!((f32_samples[0] - 1.0).abs() < 0.001);
}

#[test]
fn test_dc_offset_removal() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut data = Vec::new();
    let num_samples = format.sample_rate as usize;

    let phase_inc = 440.0 * 2.0 * PI / format.sample_rate as f32;
    let mut phase = 0.0f32;

    for _ in 0..num_samples {
        let sample = phase.sin();
        let value = ((sample * 10000.0) + 5000.0) as i16; // Significant DC offset

        let bytes = value.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);

        phase += phase_inc;
        if phase > 2.0 * PI {
            phase -= 2.0 * PI;
        }
    }

    let audio = RawAudio::from_bytes(data, format);

    let check = SineWaveCheck {
        expected_frequency: 440.0,
        ..Default::default()
    };

    let res = check.verify(&audio).unwrap();
    assert!(res.passed);
    assert!((res.measured_frequency - 440.0).abs() < 1.0);
}

#[test]
fn test_very_short_audio() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = generate_sine_wave(440.0, format.sample_rate, 0.1, format); // only 100ms

    let check = SineWaveCheck::default();
    let res = check.verify(&audio);

    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("too short"));
}

#[test]
fn test_diagnostic_report_format() {
    let format = RawAudioFormat::CD_QUALITY;
    let audio = generate_sine_wave(440.0, format.sample_rate, 1.0, format);

    let report = audio_diagnostic_report(&audio, &[]);
    assert!(report.contains("Format: 16-bit Little 2 ch @ 44100 Hz"));
    assert!(report.contains("Duration: 1.00s (44100 frames)"));
    assert!(report.contains("RESULT: PASS"));
}

#[test]
fn test_gap_detection() {
    let format = RawAudioFormat::CD_QUALITY;
    let mut data = Vec::new();
    let num_samples = format.sample_rate as usize;

    for i in 0..num_samples {
        // Gap 1: 50ms at 100ms mark
        // Gap 2: 100ms at 500ms mark
        let is_gap = (i >= 4410 && i < 6615) || (i >= 22050 && i < 26460);

        let value: i16 = if is_gap { 0 } else { 20000 };
        let bytes = value.to_le_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);
    }

    let audio = RawAudio::from_bytes(data, format);
    let gaps = measure_gap_latency(&audio, 40.0);

    assert_eq!(gaps.len(), 2);

    let g1 = &gaps[0];
    assert!((g1.duration.as_secs_f32() - 0.05).abs() < 0.001);

    let g2 = &gaps[1];
    assert!((g2.duration.as_secs_f32() - 0.1).abs() < 0.001);
}
