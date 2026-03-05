//! Audio resampling source using linear interpolation

use std::io;

use crate::audio::convert::convert_channels_into;
use crate::audio::{AudioFormat, SampleFormat};
use crate::streaming::source::AudioSource;

/// Audio source that performs sample rate conversion
pub struct ResamplingSource {
    inner: Box<dyn AudioSource>,
    input_format: AudioFormat,
    output_format: AudioFormat,
    ratio: f64,             // input_rate / output_rate
    input_phase: f64,       // Current fractional position in input
    last_samples: Vec<f32>, // Last sample from previous chunk for each channel

    // Buffers
    input_bytes_buffer: Vec<u8>,
    input_planar: Vec<Vec<f32>>,
    output_planar: Vec<Vec<f32>>,
    intermediate_buffer: Vec<f32>,
    final_buffer: Vec<f32>,
    output_bytes_buffer: Vec<u8>,
    output_offset: usize,
    eof: bool,
}

impl ResamplingSource {
    /// Create a new resampling source
    ///
    /// # Errors
    ///
    /// Returns an error if the input format is unsupported.
    pub fn new<S: AudioSource + 'static>(
        source: S,
        output_format: AudioFormat,
    ) -> io::Result<Self> {
        let input_format = source.format();

        // Ensure supported format
        match input_format.sample_format {
            SampleFormat::I16 | SampleFormat::I24 => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!(
                        "Resampling supports I16/I24 input (got {:?})",
                        input_format.sample_format
                    ),
                ));
            }
        }

        if output_format.sample_format != SampleFormat::I16 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Resampling only supports I16 output for now (got {:?})",
                    output_format.sample_format
                ),
            ));
        }

        let input_rate = f64::from(input_format.sample_rate.as_u32());
        let output_rate = f64::from(output_format.sample_rate.as_u32());
        let ratio = input_rate / output_rate;
        let channels = input_format.channels.channels() as usize;

        // Chunk size for processing
        let chunk_size = 1024;
        let input_bytes_needed = chunk_size * input_format.bytes_per_frame();

        tracing::debug!(
            "Initializing linear resampler: {} -> {} (ratio {:.4}), channels={}, chunk_size={}",
            input_rate,
            output_rate,
            ratio,
            channels,
            chunk_size
        );

        let output_capacity = {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "Conversion from usize to f64 and back is safe for small chunk sizes"
            )]
            {
                let chunk_size_f64 = chunk_size as f64;
                let cap = (chunk_size_f64 / ratio).ceil();
                cap as usize + 10
            }
        };

        Ok(Self {
            inner: Box::new(source),
            input_format,
            output_format,
            ratio,
            input_phase: 0.0,
            last_samples: vec![0.0; channels],
            input_bytes_buffer: vec![0u8; input_bytes_needed],
            input_planar: vec![Vec::with_capacity(chunk_size); channels],
            output_planar: vec![Vec::with_capacity(output_capacity); channels],
            intermediate_buffer: Vec::new(),
            final_buffer: Vec::new(),
            output_bytes_buffer: Vec::new(),
            output_offset: 0,
            eof: false,
        })
    }

    /// Process next chunk of audio
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Precision loss is acceptable for audio sample processing"
    )]
    fn process_next_chunk(&mut self) -> io::Result<bool> {
        let chunk_size = 1024; // Target input chunk size
        let bytes_per_frame = self.input_format.bytes_per_frame();
        let bytes_needed = chunk_size * bytes_per_frame;

        if self.input_bytes_buffer.len() < bytes_needed {
            self.input_bytes_buffer.resize(bytes_needed, 0);
        }

        // Read from inner source
        let mut total_read = 0;
        while total_read < bytes_needed {
            let n = self
                .inner
                .read(&mut self.input_bytes_buffer[total_read..bytes_needed])?;
            if n == 0 {
                break;
            }
            total_read += n;
        }

        if total_read == 0 {
            tracing::debug!("Resampler: Inner source EOF");
            return Ok(false); // EOF
        }

        let frames_read = total_read / bytes_per_frame;

        // De-interleave and convert to float
        self.deinterleave_input(frames_read)?;

        // Perform Linear Interpolation Resampling
        self.resample_planar(frames_read);

        // Convert output to interleaved I16 bytes
        self.interleave_and_convert_output();

        Ok(true)
    }

    fn deinterleave_input(&mut self, frames_read: usize) -> io::Result<()> {
        let channels = self.input_format.channels.channels() as usize;

        // Clear input planar buffers
        for ch in 0..channels {
            self.input_planar[ch].clear();
        }

        match self.input_format.sample_format {
            SampleFormat::I16 => {
                for i in 0..frames_read {
                    for ch in 0..channels {
                        let sample_index = i * channels + ch;
                        let byte_index = sample_index * 2;
                        let sample_i16 = i16::from_le_bytes([
                            self.input_bytes_buffer[byte_index],
                            self.input_bytes_buffer[byte_index + 1],
                        ]);
                        let sample_float = f32::from(sample_i16) / f32::from(i16::MAX);
                        self.input_planar[ch].push(sample_float);
                    }
                }
            }
            SampleFormat::I24 => {
                for i in 0..frames_read {
                    for ch in 0..channels {
                        let sample_index = i * channels + ch;
                        let byte_index = sample_index * 3;
                        let bytes = [
                            self.input_bytes_buffer[byte_index],
                            self.input_bytes_buffer[byte_index + 1],
                            self.input_bytes_buffer[byte_index + 2],
                        ];
                        let sample_i32 = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
                        #[allow(
                            clippy::cast_precision_loss,
                            reason = "i32 to f32 conversion loss is negligible for 24-bit audio"
                        )]
                        let sample_float = sample_i32 as f32 / 8_388_608.0;
                        self.input_planar[ch].push(sample_float);
                    }
                }
            }
            _ => return Err(io::Error::other("Unsupported format")),
        }
        Ok(())
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "Floating point phase calculations require truncation to usize index"
    )]
    fn resample_planar(&mut self, frames_read: usize) {
        let channels = self.input_format.channels.channels() as usize;

        // Clear output planar buffers
        for ch in 0..channels {
            self.output_planar[ch].clear();
        }

        // Process while phase < 0 (using last_samples)
        while self.input_phase < 0.0 {
            let frac = (self.input_phase - self.input_phase.floor()) as f32; // frac part

            if frames_read == 0 {
                break;
            }

            for ch in 0..channels {
                let s0 = self.last_samples[ch];
                let s1 = self.input_planar[ch][0];
                let val = s0 * (1.0 - frac) + s1 * frac;
                self.output_planar[ch].push(val);
            }
            self.input_phase += self.ratio;
        }

        // Process phase >= 0
        // We cast to usize safely here because input_phase is non-negative
        while (self.input_phase.floor() as usize) + 1 < frames_read {
            let idx = self.input_phase.floor();
            let frac = (self.input_phase - idx) as f32;
            let idx_usize = idx as usize;

            for ch in 0..channels {
                let s0 = self.input_planar[ch][idx_usize];
                let s1 = self.input_planar[ch][idx_usize + 1];
                let val = s0 * (1.0 - frac) + s1 * frac;
                self.output_planar[ch].push(val);
            }
            self.input_phase += self.ratio;
        }

        // Update last_samples for next chunk
        if frames_read > 0 {
            for ch in 0..channels {
                self.last_samples[ch] = self.input_planar[ch][frames_read - 1];
            }
        }

        // Wrap phase
        self.input_phase -= frames_read as f64;
    }

    fn interleave_and_convert_output(&mut self) {
        let channels = self.input_format.channels.channels() as usize;
        let output_frames = self.output_planar[0].len();
        let input_channels_count = channels;

        self.intermediate_buffer.clear();
        self.intermediate_buffer
            .reserve(output_frames * input_channels_count);
        for i in 0..output_frames {
            for ch in 0..input_channels_count {
                self.intermediate_buffer.push(self.output_planar[ch][i]);
            }
        }

        // Channel conversion (if needed)
        let need_conversion = self.input_format.channels != self.output_format.channels;
        if need_conversion {
            convert_channels_into(
                &self.intermediate_buffer,
                self.input_format.channels,
                self.output_format.channels,
                &mut self.final_buffer,
            );
        }

        let source_buffer = if need_conversion {
            &self.final_buffer
        } else {
            &self.intermediate_buffer
        };

        // Convert to bytes
        let output_bytes_needed = source_buffer.len() * 2;
        // Use mem::take to avoid borrow checker issues with self
        let mut output_bytes = std::mem::take(&mut self.output_bytes_buffer);
        output_bytes.clear();
        output_bytes.reserve(output_bytes_needed);

        output_bytes.extend(source_buffer.iter().flat_map(|&sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Clamped float to i16 conversion is safe"
            )]
            let value = (clamped * f32::from(i16::MAX)) as i16;
            value.to_le_bytes()
        }));

        self.output_bytes_buffer = output_bytes;
        self.output_offset = 0;
    }
}

impl AudioSource for ResamplingSource {
    fn format(&self) -> AudioFormat {
        self.output_format
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut total_written = 0;

        while total_written < buffer.len() {
            // Check if we have data available
            let available = self.output_bytes_buffer.len() - self.output_offset;

            if available > 0 {
                let to_copy = available.min(buffer.len() - total_written);
                buffer[total_written..total_written + to_copy].copy_from_slice(
                    &self.output_bytes_buffer[self.output_offset..self.output_offset + to_copy],
                );
                self.output_offset += to_copy;
                total_written += to_copy;
            } else {
                if self.eof {
                    break;
                }

                // Need more data
                match self.process_next_chunk() {
                    Ok(true) => {} // Got more data
                    Ok(false) => {
                        self.eof = true;
                        break; // EOF
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(total_written)
    }

    fn duration(&self) -> Option<std::time::Duration> {
        self.inner.duration()
    }

    fn is_seekable(&self) -> bool {
        false
    }
}
