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

mod buffer_tests {
    use crate::audio::buffer::*;

    #[test]
    fn test_write_read_simple() {
        let buffer = AudioRingBuffer::new(1024);

        let data = vec![1u8, 2, 3, 4, 5];
        let written = buffer.write(&data);
        assert_eq!(written, 5);
        assert_eq!(buffer.available(), 5);

        let mut output = vec![0u8; 5];
        let read = buffer.read(&mut output);
        assert_eq!(read, 5);
        assert_eq!(output, data);
    }

    #[test]
    fn test_wraparound() {
        let buffer = AudioRingBuffer::new(8);

        // Write 5 bytes
        buffer.write(&[1, 2, 3, 4, 5]);
        // Read 3 bytes
        let mut out = vec![0u8; 3];
        buffer.read(&mut out);
        assert_eq!(out, vec![1, 2, 3]);

        // Write 5 more (should wrap)
        buffer.write(&[6, 7, 8, 9, 10]);

        // Read all
        let mut out = vec![0u8; 7];
        let n = buffer.read(&mut out);
        assert_eq!(n, 7);
        assert_eq!(out, vec![4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_peek() {
        let buffer = AudioRingBuffer::new(1024);
        buffer.write(&[1, 2, 3, 4, 5]);

        let mut out = vec![0u8; 3];
        let peeked = buffer.peek(&mut out);
        assert_eq!(peeked, 3);
        assert_eq!(out, vec![1, 2, 3]);

        // Data should still be there
        assert_eq!(buffer.available(), 5);
    }
}

mod jitter_tests {
    use crate::audio::jitter::*;

    #[test]
    fn test_in_order_packets() {
        let mut buffer = JitterBuffer::new(2, 10);

        buffer.push(0, "packet0");
        buffer.push(1, "packet1");
        buffer.push(2, "packet2");

        assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
    }

    #[test]
    fn test_out_of_order_packets() {
        let mut buffer = JitterBuffer::new(2, 10);

        // Packets arrive out of order
        buffer.push(1, "packet1");
        buffer.push(0, "packet0");
        buffer.push(2, "packet2");

        // Should still come out in order
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet0")));
        assert!(matches!(buffer.pop(), NextPacket::Ready("packet1")));
    }

    #[test]
    fn test_duplicate_detection() {
        let mut buffer = JitterBuffer::new(2, 10);

        buffer.push(0, "packet0");
        let result = buffer.push(0, "duplicate");

        assert!(matches!(result, JitterResult::Duplicate));
    }

    #[test]
    fn test_gap_detection() {
        let mut buffer = JitterBuffer::new(1, 10);

        buffer.push(0, "packet0");
        buffer.push(2, "packet2"); // Skip 1

        buffer.pop(); // Get packet0, next expected is 1

        // Next pop should detect gap
        match buffer.pop() {
            NextPacket::Gap {
                expected: 1,
                available: 2,
            } => {}
            other => panic!("Expected Gap, got {other:?}"),
        }
    }
}

mod clock_tests {
    use crate::audio::clock::*;
    use std::time::Duration;

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
}
