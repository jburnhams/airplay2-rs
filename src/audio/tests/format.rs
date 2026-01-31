use crate::audio::convert::*;
use crate::audio::format::*;

#[test]
fn test_audio_format_bytes() {
    let format = AudioFormat::CD_QUALITY;

    assert_eq!(format.bytes_per_frame(), 4); // 2 bytes * 2 channels
    assert_eq!(format.bytes_per_second(), 176_400); // 44100 * 4
}

#[test]
fn test_duration_conversion() {
    let format = AudioFormat::CD_QUALITY;

    let duration = std::time::Duration::from_secs(1);
    let frames = format.duration_to_frames(duration);

    assert_eq!(frames, 44100);
}

#[test]
fn test_sample_format_bytes() {
    assert_eq!(SampleFormat::I16.bytes_per_sample(), 2);
    assert_eq!(SampleFormat::I24.bytes_per_sample(), 3);
    assert_eq!(SampleFormat::I32.bytes_per_sample(), 4);
    assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
}

#[test]
fn test_i16_to_f32_roundtrip() {
    let original: Vec<u8> = vec![0x00, 0x40, 0x00, 0xC0]; // ~0.5 and ~-0.5
    let f32_samples = to_f32(&original, SampleFormat::I16);
    let back = from_f32(&f32_samples, SampleFormat::I16);

    // Should be close (may have slight rounding)
    assert_eq!(original.len(), back.len());
    // Verify values
    assert_eq!(back[0], original[0]);
    assert_eq!(back[1], original[1]);
    assert_eq!(back[2], original[2]);
    assert_eq!(back[3], original[3]);
}

#[test]
fn test_i24_to_f32_roundtrip() {
    // 24-bit little endian
    // 0x400000 -> 0.5 (approx). LE bytes: [00, 00, 40]
    // 0xC00000 -> -0.5 (approx). LE bytes: [00, 00, C0]
    // Max positive: 0x7FFFFF. LE bytes: [FF, FF, 7F]
    // Max negative: 0x800000. LE bytes: [00, 00, 80]

    let original: Vec<u8> = vec![
        0x00, 0x00, 0x40, // 0.5
        0x00, 0x00, 0xC0, // -0.5
        0xFF, 0xFF, 0x7F, // Max pos
        0x00, 0x00, 0x80, // Max neg
    ];

    let f32_samples = to_f32(&original, SampleFormat::I24);

    // Check f32 values
    // 0x400000 = 4194304. 4194304 / 8388608.0 = 0.5
    assert!((f32_samples[0] - 0.5).abs() < 1e-6);

    // 0xC00000 (24-bit) -> Sign extend -> 0xFFC00000 (32-bit) = -4194304.
    // -4194304 / 8388608.0 = -0.5
    assert!((f32_samples[1] - -0.5).abs() < 1e-6);

    // Max pos: 8388607 / 8388608.0 ~= 0.99999988
    assert!((f32_samples[2] - 0.999_999_9).abs() < 1e-6);

    // Max neg: -8388608 / 8388608.0 = -1.0
    assert!((f32_samples[3] - -1.0).abs() < 1e-6);

    let back = from_f32(&f32_samples, SampleFormat::I24);

    assert_eq!(original.len(), back.len());

    // Exact byte match check
    for (i, (orig, b)) in original.iter().zip(back.iter()).enumerate() {
        assert_eq!(orig, b, "Mismatch at byte index {i}");
    }
}

#[test]
fn test_mono_to_stereo() {
    let mono = vec![1.0f32, -1.0, 0.5];
    let stereo = convert_channels(&mono, ChannelConfig::Mono, ChannelConfig::Stereo);

    assert_eq!(stereo.len(), 6);
    assert!((stereo[0] - 1.0).abs() < f32::EPSILON);
    assert!((stereo[1] - 1.0).abs() < f32::EPSILON);
    assert!((stereo[2] - -1.0).abs() < f32::EPSILON);
    assert!((stereo[3] - -1.0).abs() < f32::EPSILON);
    assert!((stereo[4] - 0.5).abs() < f32::EPSILON);
    assert!((stereo[5] - 0.5).abs() < f32::EPSILON);
}

#[test]
fn test_stereo_to_mono() {
    let stereo = vec![1.0f32, 0.5, -1.0, -0.5];
    let mono = convert_channels(&stereo, ChannelConfig::Stereo, ChannelConfig::Mono);

    assert_eq!(mono.len(), 2);
    assert!((mono[0] - 0.75).abs() < f32::EPSILON); // (1.0 + 0.5) / 2
    assert!((mono[1] - -0.75).abs() < f32::EPSILON); // (-1.0 + -0.5) / 2
}

#[test]
fn test_resample_linear_identity() {
    let input = vec![0.0f32, 0.5, 1.0, -0.5];
    let output = resample_linear(&input, 44100, 44100, 1);
    assert_eq!(input, output);
}

#[test]
fn test_resample_linear_upsample() {
    let input = vec![0.0f32, 1.0];
    // Double sample rate -> twice as many samples
    let output = resample_linear(&input, 1000, 2000, 1);

    assert_eq!(output.len(), 4);
    // Linear interpolation should give us roughly: 0.0, 0.5, 1.0, and maybe checking edge behavior
    // Actually, ratio = 0.5. output_frames = 2 / 0.5 = 4.
    // out_frame 0: in_pos 0.0. frac 0.0. sample0=0, sample1=1. res=0.0
    // out_frame 1: in_pos 0.5. frac 0.5. sample0=0, sample1=1. res=0.5
    // out_frame 2: in_pos 1.0. frac 0.0. sample0=1, sample1=1(clamped). res=1.0
    // out_frame 3: in_pos 1.5. frac 0.5. sample0=1, sample1=1(clamped). res=1.0

    assert!((output[0] - 0.0).abs() < 1e-6);
    assert!((output[1] - 0.5).abs() < 1e-6);
    assert!((output[2] - 1.0).abs() < 1e-6);
}
