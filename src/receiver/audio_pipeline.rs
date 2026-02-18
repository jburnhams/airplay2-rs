//! Audio pipeline connecting jitter buffer to output

use std::sync::{Arc, Mutex};

use crate::audio::format::{AudioCodec, AudioFormat};
use crate::audio::jitter::JitterBuffer;
use crate::audio::output::{AudioCallback, AudioOutput, AudioOutputError};

/// Audio pipeline state
pub struct AudioPipeline {
    jitter_buffer: Arc<Mutex<JitterBuffer>>,
    output: Box<dyn AudioOutput>,
    #[allow(dead_code)] // Decoder logic to be implemented later
    decoder: Option<AudioDecoder>,
    format: AudioFormat,
}

/// Audio decoder (codec-specific)
pub enum AudioDecoder {
    /// Apple Lossless Audio Codec
    Alac(AlacDecoder),
    /// Advanced Audio Coding
    Aac(AacDecoder),
    /// Raw PCM (no decoding needed)
    Pcm,
}

/// Placeholder for ALAC decoder
pub struct AlacDecoder;
/// Placeholder for AAC decoder
pub struct AacDecoder;

impl AudioPipeline {
    /// Create a new audio pipeline
    ///
    /// # Errors
    ///
    /// Returns `AudioOutputError` if the pipeline cannot be initialized.
    pub fn new(
        jitter_buffer: Arc<Mutex<JitterBuffer>>,
        output: Box<dyn AudioOutput>,
        codec: AudioCodec,
        format: AudioFormat,
    ) -> Result<Self, AudioOutputError> {
        let decoder = match codec {
            AudioCodec::Alac => Some(AudioDecoder::Alac(AlacDecoder)),
            AudioCodec::Aac => Some(AudioDecoder::Aac(AacDecoder)),
            AudioCodec::Pcm => Some(AudioDecoder::Pcm),
            AudioCodec::Opus => None, // Handle Opus or others
        };

        Ok(Self {
            jitter_buffer,
            output,
            decoder,
            format,
        })
    }

    /// Start the audio pipeline
    ///
    /// # Errors
    ///
    /// Returns `AudioOutputError` if the output cannot be opened or started.
    ///
    /// # Panics
    ///
    /// Panics if the jitter buffer lock cannot be acquired.
    pub fn start(&mut self) -> Result<(), AudioOutputError> {
        // Enforce PCM for now as decoding is not implemented
        if !matches!(self.decoder, Some(AudioDecoder::Pcm)) {
            return Err(AudioOutputError::FormatNotSupported(self.format));
        }

        self.output.open(None, self.format)?;

        let jitter = self.jitter_buffer.clone();

        let callback: AudioCallback = Box::new(move |buffer: &mut [u8]| {
            let mut jitter = jitter.lock().unwrap();

            let mut written = 0;
            while written < buffer.len() {
                if let Some(packet) = jitter.pop() {
                    let data = &packet.audio_data;

                    let to_copy = std::cmp::min(data.len(), buffer.len() - written);
                    buffer[written..written + to_copy].copy_from_slice(&data[..to_copy]);
                    written += to_copy;
                } else {
                    // Underrun - fill with silence
                    for b in &mut buffer[written..] {
                        *b = 0;
                    }
                    break;
                }
            }

            written
        });

        self.output.start(callback)
    }

    /// Stop the pipeline
    ///
    /// # Errors
    ///
    /// Returns `AudioOutputError` if the output cannot be stopped.
    pub fn stop(&mut self) -> Result<(), AudioOutputError> {
        self.output.stop()
    }

    /// Set volume
    ///
    /// # Errors
    ///
    /// Returns `AudioOutputError` if the volume cannot be set.
    pub fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
        self.output.set_volume(volume)
    }
}
