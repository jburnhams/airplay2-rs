//! AAC audio encoder using fdk-aac

use crate::audio::format::AacProfile;
use fdk_aac::enc::{AudioObjectType, BitRate, ChannelMode, Encoder, EncoderParams, Transport};
use thiserror::Error;

/// AAC encoder error
#[derive(Debug, Error)]
pub enum AacEncoderError {
    /// Initialization failed
    #[error("initialization failed")]
    Initialization,
    /// Encoding failed
    #[error("encoding failed")]
    Encoding,
}

/// AAC encoder wrapper
pub struct AacEncoder {
    encoder: Encoder,
    output_buffer: Vec<u8>,
}

impl AacEncoder {
    /// Create a new AAC encoder
    ///
    /// # Arguments
    ///
    /// * `sample_rate` - Sample rate in Hz (e.g. 44100)
    /// * `channels` - Number of channels (e.g. 2)
    /// * `bitrate` - Bitrate in bits per second (e.g. 64000)
    /// * `profile` - AAC profile (e.g. LC, ELD)
    ///
    /// # Errors
    ///
    /// Returns error if encoder cannot be initialized
    pub fn new(
        sample_rate: u32,
        channels: u32,
        bitrate: u32,
        profile: AacProfile,
    ) -> Result<Self, AacEncoderError> {
        let audio_object_type = match profile {
            AacProfile::Lc => AudioObjectType::Mpeg4LowComplexity,
            AacProfile::He => AudioObjectType::Mpeg4HeAac,
            AacProfile::HeV2 => AudioObjectType::Mpeg4HeAacV2,
            AacProfile::Eld => AudioObjectType::Mpeg4EnhancedLowDelay,
        };

        let params = EncoderParams {
            bit_rate: BitRate::Cbr(bitrate),
            transport: Transport::Raw, // Raw AAC frames for RTP
            audio_object_type,
            channels: match channels {
                1 => ChannelMode::Mono,
                2 => ChannelMode::Stereo,
                _ => return Err(AacEncoderError::Initialization),
            },
            sample_rate,
        };

        let encoder = Encoder::new(params).map_err(|_| AacEncoderError::Initialization)?;

        // Allocate buffer for worst-case output size
        // 6144 bits per channel is max theoretical size for AAC
        let buffer_size = 8192 * channels as usize;

        Ok(Self {
            encoder,
            output_buffer: vec![0u8; buffer_size],
        })
    }

    /// Encode PCM samples to AAC frame
    ///
    /// # Arguments
    ///
    /// * `pcm_samples` - Interleaved 16-bit PCM samples
    ///
    /// # Errors
    ///
    /// Returns error if encoding fails
    pub fn encode(&mut self, pcm_samples: &[i16]) -> Result<Vec<u8>, AacEncoderError> {
        let info = self
            .encoder
            .encode(pcm_samples, &mut self.output_buffer)
            .map_err(|_| AacEncoderError::Encoding)?;

        if info.output_size > 0 {
            Ok(self.output_buffer[..info.output_size].to_vec())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get frame length (samples per channel per frame)
    #[must_use]
    pub fn frame_length(&self) -> usize {
        // fdk-aac crate exposes `info().frameLength`
        // We use map/unwrap_or because info() returns Result
        self.encoder
            .info()
            .map(|i| i.frameLength as usize)
            .unwrap_or(0)
    }
}
