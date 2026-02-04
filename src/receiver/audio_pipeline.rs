//! Audio pipeline connecting jitter buffer to output

use crate::audio::output::{AudioOutput, AudioOutputError, AudioCallback};
use crate::audio::jitter::JitterBuffer;
use crate::audio::format::{AudioFormat, AudioCodec};
use std::sync::{Arc, Mutex};

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
            _ => None, // Handle Opus or others
        };

        Ok(Self {
            jitter_buffer,
            output,
            decoder,
            format,
        })
    }

    /// Start the audio pipeline
    pub fn start(&mut self) -> Result<(), AudioOutputError> {
        self.output.open(None, self.format)?;

        let jitter = self.jitter_buffer.clone();

        let callback: AudioCallback = Box::new(move |buffer: &mut [u8]| {
            let mut jitter = jitter.lock().unwrap();

            let mut written = 0;
            while written < buffer.len() {
                if let Some(packet) = jitter.pop() {
                    let data = &packet.audio_data;

                    let to_copy = std::cmp::min(
                        data.len(),
                        buffer.len() - written
                    );
                    buffer[written..written + to_copy]
                        .copy_from_slice(&data[..to_copy]);
                    written += to_copy;
                } else {
                    // Underrun - fill with silence
                    for b in buffer[written..].iter_mut() {
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
    pub fn stop(&mut self) -> Result<(), AudioOutputError> {
        self.output.stop()
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
        self.output.set_volume(volume)
    }
}
