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
    #[must_use]
    pub fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::I16 => 2,
            SampleFormat::I24 => 3,
            SampleFormat::I32 | SampleFormat::F32 => 4,
        }
    }

    /// Get bits per sample
    #[must_use]
    pub fn bits_per_sample(self) -> u8 {
        match self {
            SampleFormat::I16 => 16,
            SampleFormat::I24 => 24,
            SampleFormat::I32 | SampleFormat::F32 => 32,
        }
    }
}

/// Sample rate in Hz
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SampleRate {
    /// 44.1 kHz (CD quality)
    #[default]
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
    #[must_use]
    pub fn as_u32(self) -> u32 {
        match self {
            SampleRate::Hz44100 => 44100,
            SampleRate::Hz48000 => 48000,
            SampleRate::Hz88200 => 88200,
            SampleRate::Hz96000 => 96000,
        }
    }

    /// Create from Hz value
    #[must_use]
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

/// Channel configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelConfig {
    /// Mono (1 channel)
    Mono,
    /// Stereo (2 channels)
    #[default]
    Stereo,
    /// 5.1 surround (6 channels)
    Surround51,
    /// 7.1 surround (8 channels)
    Surround71,
}

impl ChannelConfig {
    /// Get number of channels
    #[must_use]
    pub fn channels(self) -> u8 {
        match self {
            ChannelConfig::Mono => 1,
            ChannelConfig::Stereo => 2,
            ChannelConfig::Surround51 => 6,
            ChannelConfig::Surround71 => 8,
        }
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
    #[must_use]
    pub fn new(
        sample_format: SampleFormat,
        sample_rate: SampleRate,
        channels: ChannelConfig,
    ) -> Self {
        Self {
            sample_format,
            sample_rate,
            channels,
        }
    }

    /// Get bytes per frame (all channels for one sample)
    #[must_use]
    pub fn bytes_per_frame(self) -> usize {
        self.sample_format.bytes_per_sample() * usize::from(self.channels.channels())
    }

    /// Get bytes per second
    #[must_use]
    pub fn bytes_per_second(self) -> usize {
        self.bytes_per_frame() * self.sample_rate.as_u32() as usize
    }

    /// Calculate duration for given number of frames
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn frames_to_duration(self, frames: usize) -> std::time::Duration {
        std::time::Duration::from_secs_f64(frames as f64 / f64::from(self.sample_rate.as_u32()))
    }

    /// Calculate frames for given duration
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[must_use]
    pub fn duration_to_frames(self, duration: std::time::Duration) -> usize {
        (duration.as_secs_f64() * f64::from(self.sample_rate.as_u32())) as usize
    }

    /// Calculate bytes for given duration
    #[must_use]
    pub fn duration_to_bytes(self, duration: std::time::Duration) -> usize {
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
    /// Enhanced Low Delay AAC
    AacEld,
    /// Opus (for low-latency applications)
    Opus,
}

/// Codec-specific parameters
#[derive(Debug, Clone)]
pub enum CodecParams {
    /// PCM parameters
    Pcm {
        /// Audio format
        format: AudioFormat,
        /// Is big endian
        big_endian: bool,
    },
    /// ALAC parameters
    Alac {
        /// Audio format
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
    // High Efficiency (SBR)
    // He,
    // High Efficiency v2 (SBR + PS)
    // HeV2,
    /// Enhanced Low Delay
    Eld,
}
