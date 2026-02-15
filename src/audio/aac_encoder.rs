//! AAC audio encoder using fdk-aac

use fdk_aac::enc::{BitRate, ChannelMode, Encoder, EncoderParams, Transport};
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
    ///
    /// # Errors
    ///
    /// Returns error if encoder cannot be initialized
    pub fn new(sample_rate: u32, channels: u32, bitrate: u32) -> Result<Self, AacEncoderError> {
        let params = EncoderParams {
            bit_rate: BitRate::Cbr(bitrate),
            transport: Transport::Adts, // ADTS for compatibility
            audio_object_type: fdk_aac::enc::AudioObjectType::Mpeg4LowComplexity,
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
}
