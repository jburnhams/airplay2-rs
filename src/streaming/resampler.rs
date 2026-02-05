//! Audio resampling source using `rubato`

use crate::audio::{AudioFormat, SampleFormat, convert::convert_channels};
use crate::streaming::source::AudioSource;
use rubato::{FftFixedIn, Resampler};
use std::io;

/// Audio source that performs sample rate conversion
pub struct ResamplingSource<S: AudioSource> {
    inner: S,
    #[allow(dead_code)]
    input_format: AudioFormat,
    output_format: AudioFormat,
    resampler: FftFixedIn<f32>,
    input_buffer: Vec<Vec<f32>>,
    output_buffer: Vec<Vec<f32>>,
    input_bytes_buffer: Vec<u8>,
    output_bytes_buffer: Vec<u8>,
    output_offset: usize,
    eof: bool,
}

impl<S: AudioSource> ResamplingSource<S> {
    /// Create a new resampling source
    ///
    /// # Errors
    ///
    /// Returns an error if the input format is unsupported (e.g. not I16) or if
    /// the resampler cannot be initialized.
    pub fn new(source: S, output_format: AudioFormat) -> io::Result<Self> {
        let input_format = source.format();

        // Ensure supported format
        if input_format.sample_format != SampleFormat::I16 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Resampling only supports I16 input for now",
            ));
        }
        if output_format.sample_format != SampleFormat::I16 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Resampling only supports I16 output for now",
            ));
        }

        // Initialize rubato resampler
        let input_rate = input_format.sample_rate.as_u32() as usize;
        let output_rate = output_format.sample_rate.as_u32() as usize;
        let channels = input_format.channels.channels() as usize;
        let chunk_size = 1024; // Reasonable chunk size

        let resampler = FftFixedIn::<f32>::new(input_rate, output_rate, chunk_size, 2, channels)
            .map_err(|e| io::Error::other(e.to_string()))?;

        // Pre-allocate buffers
        let input_buffer = resampler.input_buffer_allocate(true);
        let output_buffer = resampler.output_buffer_allocate(true);
        let input_frames_needed = resampler.input_frames_next();
        let input_bytes_needed = input_frames_needed * input_format.bytes_per_frame();

        Ok(Self {
            inner: source,
            input_format,
            output_format,
            resampler,
            input_buffer,
            output_buffer,
            input_bytes_buffer: vec![0u8; input_bytes_needed],
            output_bytes_buffer: Vec::new(),
            output_offset: 0,
            eof: false,
        })
    }

    /// Process next chunk of audio
    #[allow(clippy::cast_possible_truncation)]
    fn process_next_chunk(&mut self) -> io::Result<bool> {
        let input_frames_needed = self.resampler.input_frames_next();
        let bytes_needed = input_frames_needed * self.input_format.bytes_per_frame();

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
            return Ok(false); // EOF
        }

        // Zero-pad if partial read (EOF approached)
        if total_read < bytes_needed {
            self.input_bytes_buffer[total_read..].fill(0);
        }

        // Convert input bytes to planar f32
        // Assuming I16 input
        let channels = self.input_format.channels.channels() as usize;

        for ch in 0..channels {
            self.input_buffer[ch].clear();
            // Capacity is usually sufficient as we allocated with chunk_size
        }

        // De-interleave and convert
        for i in 0..input_frames_needed {
            for ch in 0..channels {
                let sample_index = i * channels + ch;
                let byte_index = sample_index * 2;

                // Read i16
                let sample_i16 = i16::from_le_bytes([
                    self.input_bytes_buffer[byte_index],
                    self.input_bytes_buffer[byte_index + 1],
                ]);

                let sample_float = f32::from(sample_i16) / f32::from(i16::MAX);
                self.input_buffer[ch].push(sample_float);
            }
        }

        // Resample
        let (_, output_frames) = self
            .resampler
            .process_into_buffer(&self.input_buffer, &mut self.output_buffer, None)
            .map_err(|e| io::Error::other(e.to_string()))?;

        // Convert planar f32 back to interleaved bytes
        // Assuming I16 output
        let input_channels_count = self.input_format.channels.channels() as usize;

        // 1. Interleave resampled data (in input_channels config)
        let mut interleaved_f32 = Vec::with_capacity(output_frames * input_channels_count);
        for i in 0..output_frames {
            for ch in 0..input_channels_count {
                interleaved_f32.push(self.output_buffer[ch][i]);
            }
        }

        // 2. Convert channels if needed
        #[allow(clippy::if_not_else)]
        let final_f32 = if self.input_format.channels != self.output_format.channels {
            convert_channels(
                &interleaved_f32,
                self.input_format.channels,
                self.output_format.channels,
            )
        } else {
            interleaved_f32
        };

        // 3. Convert to bytes
        let output_bytes_needed = final_f32.len() * 2; // I16 = 2 bytes

        self.output_bytes_buffer.clear();
        self.output_bytes_buffer.reserve(output_bytes_needed);

        for sample in final_f32 {
            let clamped = sample.clamp(-1.0, 1.0);
            let value = (clamped * f32::from(i16::MAX)) as i16;
            let bytes = value.to_le_bytes();
            self.output_bytes_buffer.extend_from_slice(&bytes);
        }

        self.output_offset = 0;
        Ok(true)
    }
}

impl<S: AudioSource> AudioSource for ResamplingSource<S> {
    fn format(&self) -> AudioFormat {
        self.output_format
    }

    #[allow(clippy::needless_continue)]
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
                    Ok(true) => continue, // Got more data
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
