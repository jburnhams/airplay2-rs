//! Audio format conversion utilities

use super::format::{ChannelConfig, SampleFormat};

/// Convert between sample formats
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn convert_samples(
    input: &[u8],
    input_format: SampleFormat,
    output_format: SampleFormat,
) -> Vec<u8> {
    if input_format == output_format {
        return input.to_vec();
    }

    // Convert to f32 as intermediate, then to output format
    let samples_f32 = to_f32(input, input_format);
    from_f32(&samples_f32, output_format)
}

/// Convert bytes to f32 samples
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn to_f32(input: &[u8], format: SampleFormat) -> Vec<f32> {
    match format {
        SampleFormat::I16 => input
            .chunks_exact(2)
            .map(|bytes| {
                let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                f32::from(sample) / f32::from(i16::MAX)
            })
            .collect(),
        SampleFormat::I24 => input
            .chunks_exact(3)
            .map(|bytes| {
                // Load into upper 24 bits for sign extension
                let sample = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
                sample as f32 / 8_388_608.0
            })
            .collect(),
        SampleFormat::I32 => input
            .chunks_exact(4)
            .map(|bytes| {
                let sample = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                sample as f32 / i32::MAX as f32
            })
            .collect(),
        SampleFormat::F32 => input
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect(),
    }
}

/// Convert f32 samples to bytes in target format
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn from_f32(input: &[f32], format: SampleFormat) -> Vec<u8> {
    match format {
        SampleFormat::I16 => input
            .iter()
            .flat_map(|&sample| {
                let clamped = sample.clamp(-1.0, 1.0);
                let value = (clamped * f32::from(i16::MAX)) as i16;
                value.to_le_bytes()
            })
            .collect(),
        SampleFormat::I24 => input
            .iter()
            .flat_map(|&sample| {
                let clamped = sample.clamp(-1.0, 1.0);
                // Scale by 2^23, but clamp to max 24-bit value to avoid wrapping
                let scaled = clamped * 8_388_608.0;
                let value = if scaled >= 8_388_607.0 {
                    8_388_607
                } else {
                    scaled as i32
                };
                let bytes = value.to_le_bytes();
                [bytes[0], bytes[1], bytes[2]]
            })
            .collect(),
        SampleFormat::I32 => input
            .iter()
            .flat_map(|&sample| {
                let clamped = sample.clamp(-1.0, 1.0);
                let value = (clamped * i32::MAX as f32) as i32;
                value.to_le_bytes()
            })
            .collect(),
        SampleFormat::F32 => input
            .iter()
            .flat_map(|&sample| sample.to_le_bytes())
            .collect(),
    }
}

/// Convert channel configuration
#[must_use]
pub fn convert_channels(
    input: &[f32],
    input_channels: ChannelConfig,
    output_channels: ChannelConfig,
) -> Vec<f32> {
    let in_ch = usize::from(input_channels.channels());
    let out_ch = usize::from(output_channels.channels());

    if in_ch == out_ch {
        return input.to_vec();
    }

    let frames = input.len() / in_ch;
    let mut output = vec![0.0f32; frames * out_ch];

    for frame in 0..frames {
        let in_start = frame * in_ch;
        let out_start = frame * out_ch;

        match (input_channels, output_channels) {
            (ChannelConfig::Mono, ChannelConfig::Stereo) => {
                // Mono to stereo: duplicate
                output[out_start] = input[in_start];
                output[out_start + 1] = input[in_start];
            }
            (ChannelConfig::Stereo, ChannelConfig::Mono) => {
                // Stereo to mono: average
                output[out_start] = (input[in_start] + input[in_start + 1]) * 0.5;
            }
            _ => {
                // Generic: copy what we can, zero the rest
                let count = out_ch.min(in_ch);
                output[out_start..out_start + count]
                    .copy_from_slice(&input[in_start..in_start + count]);
            }
        }
    }

    output
}

/// Simple sample rate conversion (linear interpolation)
///
/// For production use, consider a proper resampler like rubato
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_sign_loss)]
#[must_use]
pub fn resample_linear(input: &[f32], input_rate: u32, output_rate: u32, channels: u8) -> Vec<f32> {
    if input_rate == output_rate {
        return input.to_vec();
    }

    let channels = channels as usize;
    let input_frames = input.len() / channels;
    let ratio = f64::from(input_rate) / f64::from(output_rate);
    let output_frames = (input_frames as f64 / ratio) as usize;

    let mut output = vec![0.0f32; output_frames * channels];

    for out_frame in 0..output_frames {
        let in_pos = out_frame as f64 * ratio;
        let in_frame = in_pos as usize;
        let frac = (in_pos - in_frame as f64) as f32;

        for ch in 0..channels {
            let idx0 = in_frame * channels + ch;
            let idx1 = (in_frame + 1).min(input_frames - 1) * channels + ch;

            let sample0 = input[idx0];
            let sample1 = input[idx1];

            output[out_frame * channels + ch] = sample0 * (1.0 - frac) + sample1 * frac;
        }
    }

    output
}
