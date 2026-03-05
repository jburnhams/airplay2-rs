//! Audio source abstraction

use std::io;

use crate::audio::AudioFormat;

/// Audio source that provides PCM samples
pub trait AudioSource: Send {
    /// Get the audio format
    fn format(&self) -> AudioFormat;

    /// Read PCM samples into buffer
    ///
    /// Returns the number of bytes read, or 0 for EOF
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize>;

    /// Get total duration if known
    fn duration(&self) -> Option<std::time::Duration> {
        None
    }

    /// Get current position
    fn position(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }

    /// Seek to position (if supported)
    ///
    /// # Errors
    ///
    /// Returns an error if seek is not supported or fails
    fn seek(&mut self, _position: std::time::Duration) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "seek not supported",
        ))
    }

    /// Check if source is seekable
    fn is_seekable(&self) -> bool {
        false
    }
}

/// Audio source from a byte slice
pub struct SliceSource {
    data: Vec<u8>,
    position: usize,
    format: AudioFormat,
}

impl SliceSource {
    /// Create from raw PCM data
    #[must_use]
    pub fn new(data: Vec<u8>, format: AudioFormat) -> Self {
        Self {
            data,
            position: 0,
            format,
        }
    }

    /// Create from i16 samples
    #[must_use]
    pub fn from_i16(samples: &[i16], format: AudioFormat) -> Self {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        Self::new(data, format)
    }
}

impl AudioSource for SliceSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.data.len() - self.position;
        let to_read = buffer.len().min(remaining);

        if to_read == 0 {
            return Ok(0);
        }

        buffer[..to_read].copy_from_slice(&self.data[self.position..self.position + to_read]);
        self.position += to_read;

        Ok(to_read)
    }

    fn duration(&self) -> Option<std::time::Duration> {
        let frames = self.data.len() / self.format.bytes_per_frame();
        Some(self.format.frames_to_duration(frames))
    }

    fn position(&self) -> std::time::Duration {
        let frames = self.position / self.format.bytes_per_frame();
        self.format.frames_to_duration(frames)
    }

    fn seek(&mut self, position: std::time::Duration) -> io::Result<()> {
        let frames = self.format.duration_to_frames(position);
        let byte_pos = frames * self.format.bytes_per_frame();

        if byte_pos > self.data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek beyond end",
            ));
        }

        self.position = byte_pos;
        Ok(())
    }

    fn is_seekable(&self) -> bool {
        true
    }
}

/// Audio source from a callback function
pub struct CallbackSource<F> {
    callback: F,
    format: AudioFormat,
}

impl<F> CallbackSource<F>
where
    F: FnMut(&mut [u8]) -> io::Result<usize> + Send,
{
    /// Create from callback
    pub fn new(format: AudioFormat, callback: F) -> Self {
        Self { callback, format }
    }
}

impl<F> AudioSource for CallbackSource<F>
where
    F: FnMut(&mut [u8]) -> io::Result<usize> + Send,
{
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        (self.callback)(buffer)
    }
}

/// Silence generator
pub struct SilenceSource {
    format: AudioFormat,
}

impl SilenceSource {
    /// Create a new silence source
    #[must_use]
    pub fn new(format: AudioFormat) -> Self {
        Self { format }
    }
}

impl AudioSource for SilenceSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        buffer.fill(0);
        Ok(buffer.len())
    }
}
