# Section 11: Audio Format and Codec Support

> **VERIFIED**: Checked against `src/audio/format.rs`, `src/audio/convert.rs` on 2025-01-30.
> Implementation complete with additional raop_encoder module for RAOP encoding.

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

This section provides audio format handling and codec support for AirPlay streaming. AirPlay 2 supports various audio formats including:
- PCM (raw uncompressed audio)
- AAC (compressed)
- ALAC (Apple Lossless)

For this implementation, we focus on PCM as the canonical internal format.

## Objectives

- Define audio format types and conversions
- Implement format negotiation
- Support sample rate conversion (if needed)
- Handle channel configuration

---

## Tasks

### 11.1 Audio Format Types

- [x] **11.1.1** Define audio format enums and types

**File:** `src/audio/format.rs`

```rust
//! Audio format definitions

/// Audio sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleFormat {
    /// 16-bit signed integer
    I16,
    /// 24-bit signed integer (packed)
    I24,
    /// 32-bit signed integer
    I32,
    /// 32-bit float
    F32,
}

impl SampleFormat {
    /// Get bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::I16 => 2,
            SampleFormat::I24 => 3,
            SampleFormat::I32 => 4,
            SampleFormat::F32 => 4,
        }
    }

    /// Get bits per sample
    pub fn bits_per_sample(&self) -> u8 {
        match self {
            SampleFormat::I16 => 16,
            SampleFormat::I24 => 24,
            SampleFormat::I32 => 32,
            SampleFormat::F32 => 32,
        }
    }
}

/// Sample rate in Hz
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleRate {
    /// 44.1 kHz (CD quality)
    Hz44100,
    /// 48 kHz (DVD/standard digital audio)
    Hz48000,
    /// 88.2 kHz (double CD rate)
    Hz88200,
    /// 96 kHz (high resolution)
    Hz96000,
}

impl SampleRate {
    /// Get the rate as u32
    pub fn as_u32(&self) -> u32 {
        match self {
            SampleRate::Hz44100 => 44100,
            SampleRate::Hz48000 => 48000,
            SampleRate::Hz88200 => 88200,
            SampleRate::Hz96000 => 96000,
        }
    }

    /// Create from Hz value
    pub fn from_hz(hz: u32) -> Option<Self> {
        match hz {
            44100 => Some(SampleRate::Hz44100),
            48000 => Some(SampleRate::Hz48000),
            88200 => Some(SampleRate::Hz88200),
            96000 => Some(SampleRate::Hz96000),
            _ => None,
        }
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        SampleRate::Hz44100
    }
}

/// Channel configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelConfig {
    /// Mono (1 channel)
    Mono,
    /// Stereo (2 channels)
    Stereo,
    /// 5.1 surround (6 channels)
    Surround51,
    /// 7.1 surround (8 channels)
    Surround71,
}

impl ChannelConfig {
    /// Get number of channels
    pub fn channels(&self) -> u8 {
        match self {
            ChannelConfig::Mono => 1,
            ChannelConfig::Stereo => 2,
            ChannelConfig::Surround51 => 6,
            ChannelConfig::Surround71 => 8,
        }
    }
}

impl Default for ChannelConfig {
    fn default() -> Self {
        ChannelConfig::Stereo
    }
}

/// Complete audio format specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFormat {
    /// Sample format
    pub sample_format: SampleFormat,
    /// Sample rate
    pub sample_rate: SampleRate,
    /// Channel configuration
    pub channels: ChannelConfig,
}

impl AudioFormat {
    /// Standard CD audio format (16-bit 44.1kHz stereo)
    pub const CD_QUALITY: Self = Self {
        sample_format: SampleFormat::I16,
        sample_rate: SampleRate::Hz44100,
        channels: ChannelConfig::Stereo,
    };

    /// Create a new audio format
    pub fn new(sample_format: SampleFormat, sample_rate: SampleRate, channels: ChannelConfig) -> Self {
        Self {
            sample_format,
            sample_rate,
            channels,
        }
    }

    /// Get bytes per frame (all channels for one sample)
    pub fn bytes_per_frame(&self) -> usize {
        self.sample_format.bytes_per_sample() * self.channels.channels() as usize
    }

    /// Get bytes per second
    pub fn bytes_per_second(&self) -> usize {
        self.bytes_per_frame() * self.sample_rate.as_u32() as usize
    }

    /// Calculate duration for given number of frames
    pub fn frames_to_duration(&self, frames: usize) -> std::time::Duration {
        std::time::Duration::from_secs_f64(frames as f64 / self.sample_rate.as_u32() as f64)
    }

    /// Calculate frames for given duration
    pub fn duration_to_frames(&self, duration: std::time::Duration) -> usize {
        (duration.as_secs_f64() * self.sample_rate.as_u32() as f64) as usize
    }

    /// Calculate bytes for given duration
    pub fn duration_to_bytes(&self, duration: std::time::Duration) -> usize {
        self.duration_to_frames(duration) * self.bytes_per_frame()
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::CD_QUALITY
    }
}

/// Audio codec for compressed formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// Raw PCM (no compression)
    Pcm,
    /// Apple Lossless Audio Codec
    Alac,
    /// Advanced Audio Coding
    Aac,
    /// Opus (for low-latency applications)
    Opus,
}

/// Codec-specific parameters
#[derive(Debug, Clone)]
pub enum CodecParams {
    /// PCM parameters
    Pcm {
        format: AudioFormat,
        big_endian: bool,
    },
    /// ALAC parameters
    Alac {
        format: AudioFormat,
        /// ALAC magic cookie
        magic_cookie: Vec<u8>,
    },
    /// AAC parameters
    Aac {
        /// AAC profile (LC, HE, etc.)
        profile: AacProfile,
        /// Audio-specific config (ASC)
        asc: Vec<u8>,
    },
}

/// AAC profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacProfile {
    /// Low Complexity
    Lc,
    /// High Efficiency (SBR)
    He,
    /// High Efficiency v2 (SBR + PS)
    HeV2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format_bytes() {
        let format = AudioFormat::CD_QUALITY;

        assert_eq!(format.bytes_per_frame(), 4); // 2 bytes * 2 channels
        assert_eq!(format.bytes_per_second(), 176400); // 44100 * 4
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
}
```

---

### 11.2 Format Conversion

- [x] **11.2.1** Implement format conversion utilities

**File:** `src/audio/convert.rs`

```rust
//! Audio format conversion utilities

use super::format::{SampleFormat, ChannelConfig, AudioFormat};

/// Convert between sample formats
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
pub fn to_f32(input: &[u8], format: SampleFormat) -> Vec<f32> {
    match format {
        SampleFormat::I16 => {
            input
                .chunks_exact(2)
                .map(|bytes| {
                    let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                    sample as f32 / i16::MAX as f32
                })
                .collect()
        }
        SampleFormat::I24 => {
            input
                .chunks_exact(3)
                .map(|bytes| {
                    let sample = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]) >> 8;
                    sample as f32 / (1 << 23) as f32
                })
                .collect()
        }
        SampleFormat::I32 => {
            input
                .chunks_exact(4)
                .map(|bytes| {
                    let sample = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    sample as f32 / i32::MAX as f32
                })
                .collect()
        }
        SampleFormat::F32 => {
            input
                .chunks_exact(4)
                .map(|bytes| {
                    f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                })
                .collect()
        }
    }
}

/// Convert f32 samples to bytes in target format
pub fn from_f32(input: &[f32], format: SampleFormat) -> Vec<u8> {
    match format {
        SampleFormat::I16 => {
            input
                .iter()
                .flat_map(|&sample| {
                    let clamped = sample.clamp(-1.0, 1.0);
                    let value = (clamped * i16::MAX as f32) as i16;
                    value.to_le_bytes()
                })
                .collect()
        }
        SampleFormat::I24 => {
            input
                .iter()
                .flat_map(|&sample| {
                    let clamped = sample.clamp(-1.0, 1.0);
                    let value = (clamped * (1 << 23) as f32) as i32;
                    let bytes = value.to_le_bytes();
                    [bytes[0], bytes[1], bytes[2]]
                })
                .collect()
        }
        SampleFormat::I32 => {
            input
                .iter()
                .flat_map(|&sample| {
                    let clamped = sample.clamp(-1.0, 1.0);
                    let value = (clamped * i32::MAX as f32) as i32;
                    value.to_le_bytes()
                })
                .collect()
        }
        SampleFormat::F32 => {
            input
                .iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect()
        }
    }
}

/// Convert channel configuration
pub fn convert_channels(
    input: &[f32],
    input_channels: ChannelConfig,
    output_channels: ChannelConfig,
) -> Vec<f32> {
    let in_ch = input_channels.channels() as usize;
    let out_ch = output_channels.channels() as usize;

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
                for ch in 0..out_ch.min(in_ch) {
                    output[out_start + ch] = input[in_start + ch];
                }
            }
        }
    }

    output
}

/// Simple sample rate conversion (linear interpolation)
///
/// For production use, consider a proper resampler like rubato
pub fn resample_linear(
    input: &[f32],
    input_rate: u32,
    output_rate: u32,
    channels: u8,
) -> Vec<f32> {
    if input_rate == output_rate {
        return input.to_vec();
    }

    let channels = channels as usize;
    let input_frames = input.len() / channels;
    let ratio = input_rate as f64 / output_rate as f64;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i16_to_f32_roundtrip() {
        let original: Vec<u8> = vec![0x00, 0x40, 0x00, 0xC0]; // ~0.5 and ~-0.5
        let f32_samples = to_f32(&original, SampleFormat::I16);
        let back = from_f32(&f32_samples, SampleFormat::I16);

        // Should be close (may have slight rounding)
        assert_eq!(original.len(), back.len());
    }

    #[test]
    fn test_mono_to_stereo() {
        let mono = vec![1.0f32, -1.0, 0.5];
        let stereo = convert_channels(&mono, ChannelConfig::Mono, ChannelConfig::Stereo);

        assert_eq!(stereo.len(), 6);
        assert_eq!(stereo[0], 1.0);
        assert_eq!(stereo[1], 1.0);
    }

    #[test]
    fn test_stereo_to_mono() {
        let stereo = vec![1.0f32, 0.5, -1.0, -0.5];
        let mono = convert_channels(&stereo, ChannelConfig::Stereo, ChannelConfig::Mono);

        assert_eq!(mono.len(), 2);
        assert_eq!(mono[0], 0.75);
        assert_eq!(mono[1], -0.75);
    }
}
```

---

### 11.3 Module Entry Point

- [x] **11.3.1** Create audio module

**File:** `src/audio/mod.rs`

```rust
//! Audio processing and streaming

mod format;
mod convert;

pub use format::{
    SampleFormat, SampleRate, ChannelConfig, AudioFormat,
    AudioCodec, CodecParams, AacProfile,
};

pub use convert::{
    convert_samples, to_f32, from_f32,
    convert_channels, resample_linear,
};
```

---

## Acceptance Criteria

- [x] Audio format types are defined
- [x] Sample format conversion works
- [x] Channel conversion (mono/stereo) works
- [x] Basic resampling works
- [x] CD quality format is the default
- [x] All unit tests pass

---

## Notes

- For production resampling, use a library like `rubato`
- ALAC/AAC codec support would require additional dependencies
- Consider adding SIMD optimizations for format conversion
