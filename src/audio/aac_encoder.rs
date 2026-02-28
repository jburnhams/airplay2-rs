//! AAC audio encoder using fdk-aac

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
    /// * `aot` - Audio Object Type (e.g. LC, ELD)
    ///
    /// # Errors
    ///
    /// Returns error if encoder cannot be initialized
    pub fn new(
        sample_rate: u32,
        channels: u32,
        bitrate: u32,
        aot: AudioObjectType,
    ) -> Result<Self, AacEncoderError> {
        let params = EncoderParams {
            bit_rate: BitRate::Cbr(bitrate),
            transport: Transport::Raw, // Raw AAC frames for RTP
            audio_object_type: aot,
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

    /// Get Audio Specific Config (ASC)
    ///
    /// returns the raw ASC bytes if available
    #[must_use]
    pub fn get_asc(&self) -> Option<Vec<u8>> {
        if let Ok(info) = self.encoder.info() {
            if info.confSize > 0 {
                // confBuf is fixed-size array in EncoderInfo
                // We need to slice it to confSize
                let size = info.confSize as usize;
                if size <= info.confBuf.len() {
                    return Some(info.confBuf[..size].to_vec());
                }
            }
        }
        None
    }

    /// Get frame length (samples per channel)
    #[must_use]
    pub fn get_frame_length(&self) -> Option<u32> {
        self.encoder.info().ok().map(|info| info.frameLength)
    }
}
