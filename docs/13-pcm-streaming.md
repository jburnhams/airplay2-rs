# Section 13: PCM Audio Streaming

## Dependencies
- **Section 06**: RTP Protocol (must be complete)
- **Section 10**: Connection Management (must be complete)
- **Section 11**: Audio Formats (must be complete)
- **Section 12**: Audio Buffer and Timing (must be complete)

## Overview

This section implements raw PCM audio streaming to AirPlay devices. It handles:
- Encoding PCM data into RTP packets
- Managing audio buffer flow
- Handling timing synchronization
- Supporting both realtime and buffered modes

## Objectives

- Implement PCM audio source abstraction
- Create streaming pipeline from source to RTP
- Handle buffer underruns/overruns
- Support playback control (pause/resume)

---

## Tasks

### 13.1 Audio Source Trait

- [ ] **13.1.1** Define audio source abstraction

**File:** `src/streaming/source.rs`

```rust
//! Audio source abstraction

use crate::audio::{AudioFormat, SampleFormat, ChannelConfig};
use std::io;

/// Audio source that provides PCM samples
pub trait AudioSource: Send {
    /// Get the audio format
    fn format(&self) -> AudioFormat;

    /// Read PCM samples into buffer
    ///
    /// Returns the number of bytes read, or 0 for EOF
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
    fn seek(&mut self, _position: std::time::Duration) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "seek not supported"))
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
    pub fn new(data: Vec<u8>, format: AudioFormat) -> Self {
        Self {
            data,
            position: 0,
            format,
        }
    }

    /// Create from i16 samples
    pub fn from_i16(samples: &[i16], format: AudioFormat) -> Self {
        let data: Vec<u8> = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_source() {
        let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut source = SliceSource::new(data.clone(), AudioFormat::CD_QUALITY);

        let mut buffer = vec![0u8; 4];
        let n = source.read(&mut buffer).unwrap();
        assert_eq!(n, 4);
        assert_eq!(buffer, vec![1, 2, 3, 4]);

        let n = source.read(&mut buffer).unwrap();
        assert_eq!(n, 4);
        assert_eq!(buffer, vec![5, 6, 7, 8]);

        let n = source.read(&mut buffer).unwrap();
        assert_eq!(n, 0); // EOF
    }

    #[test]
    fn test_silence_source() {
        let mut source = SilenceSource::new(AudioFormat::CD_QUALITY);

        let mut buffer = vec![255u8; 100];
        let n = source.read(&mut buffer).unwrap();

        assert_eq!(n, 100);
        assert!(buffer.iter().all(|&b| b == 0));
    }
}
```

---

### 13.2 PCM Streamer

- [ ] **13.2.1** Implement PCM streaming pipeline

**File:** `src/streaming/pcm.rs`

```rust
//! PCM audio streaming to AirPlay devices

use super::source::AudioSource;
use crate::audio::{AudioFormat, AudioRingBuffer};
use crate::protocol::rtp::RtpCodec;
use crate::connection::ConnectionManager;
use crate::error::AirPlayError;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use std::time::Duration;

/// PCM streamer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamerState {
    /// Idle, not streaming
    Idle,
    /// Buffering audio
    Buffering,
    /// Actively streaming
    Streaming,
    /// Paused
    Paused,
    /// Stream ended
    Finished,
    /// Error occurred
    Error,
}

/// PCM audio streamer
pub struct PcmStreamer {
    /// Connection manager
    connection: Arc<ConnectionManager>,
    /// Audio format
    format: AudioFormat,
    /// RTP codec
    rtp_codec: Mutex<RtpCodec>,
    /// Audio buffer
    buffer: Arc<AudioRingBuffer>,
    /// Current state
    state: RwLock<StreamerState>,
    /// Command sender
    cmd_tx: mpsc::Sender<StreamerCommand>,
    /// Command receiver
    cmd_rx: Mutex<mpsc::Receiver<StreamerCommand>>,
}

/// Commands for the streamer
#[derive(Debug)]
enum StreamerCommand {
    /// Start streaming from source
    Start,
    /// Pause streaming
    Pause,
    /// Resume streaming
    Resume,
    /// Stop streaming
    Stop,
    /// Seek to position
    Seek(Duration),
}

impl PcmStreamer {
    /// Frames per RTP packet (standard AirPlay)
    pub const FRAMES_PER_PACKET: usize = 352;

    /// Create a new PCM streamer
    pub fn new(connection: Arc<ConnectionManager>, format: AudioFormat) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);

        // Buffer for ~500ms of audio
        let buffer_size = format.duration_to_bytes(Duration::from_millis(500));
        let buffer = Arc::new(AudioRingBuffer::new(buffer_size));

        // SSRC for RTP
        let ssrc = rand::random::<u32>();
        let rtp_codec = RtpCodec::new(ssrc);

        Self {
            connection,
            format,
            rtp_codec: Mutex::new(rtp_codec),
            buffer,
            state: RwLock::new(StreamerState::Idle),
            cmd_tx,
            cmd_rx: Mutex::new(cmd_rx),
        }
    }

    /// Get current state
    pub async fn state(&self) -> StreamerState {
        *self.state.read().await
    }

    /// Start streaming from an audio source
    pub async fn stream<S: AudioSource + 'static>(
        &self,
        mut source: S,
    ) -> Result<(), AirPlayError> {
        // Check format compatibility
        if source.format() != self.format {
            return Err(AirPlayError::InvalidParameter {
                name: "format".to_string(),
                message: "Source format doesn't match streamer format".to_string(),
            });
        }

        *self.state.write().await = StreamerState::Buffering;

        // Fill buffer initially
        self.fill_buffer(&mut source).await?;

        *self.state.write().await = StreamerState::Streaming;

        // Start streaming loop
        self.streaming_loop(source).await
    }

    /// Fill the audio buffer from source
    async fn fill_buffer<S: AudioSource>(&self, source: &mut S) -> Result<(), AirPlayError> {
        let bytes_per_packet = Self::FRAMES_PER_PACKET * self.format.bytes_per_frame();
        let mut temp_buffer = vec![0u8; bytes_per_packet * 4];

        while !self.buffer.is_ready() {
            let n = source.read(&mut temp_buffer)?;
            if n == 0 {
                break; // EOF
            }
            self.buffer.write(&temp_buffer[..n]);
        }

        Ok(())
    }

    /// Main streaming loop
    async fn streaming_loop<S: AudioSource>(
        &self,
        mut source: S,
    ) -> Result<(), AirPlayError> {
        let bytes_per_packet = Self::FRAMES_PER_PACKET * self.format.bytes_per_frame();
        let packet_duration = self.format.frames_to_duration(Self::FRAMES_PER_PACKET);

        let mut packet_data = vec![0u8; bytes_per_packet];
        let mut cmd_rx = self.cmd_rx.lock().await;

        loop {
            // Check for commands
            match cmd_rx.try_recv() {
                Ok(StreamerCommand::Pause) => {
                    *self.state.write().await = StreamerState::Paused;
                    // Wait for resume
                    loop {
                        match cmd_rx.recv().await {
                            Some(StreamerCommand::Resume) => break,
                            Some(StreamerCommand::Stop) => {
                                *self.state.write().await = StreamerState::Idle;
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                    *self.state.write().await = StreamerState::Streaming;
                }
                Ok(StreamerCommand::Stop) => {
                    *self.state.write().await = StreamerState::Idle;
                    return Ok(());
                }
                Ok(StreamerCommand::Seek(pos)) => {
                    if source.is_seekable() {
                        source.seek(pos)?;
                        self.buffer.clear();
                        self.fill_buffer(&mut source).await?;
                    }
                }
                _ => {}
            }

            // Read from buffer
            let bytes_read = self.buffer.read(&mut packet_data);

            if bytes_read == 0 {
                // Try to fill buffer
                let mut temp = vec![0u8; bytes_per_packet * 2];
                let n = source.read(&mut temp)?;

                if n == 0 {
                    // EOF
                    *self.state.write().await = StreamerState::Finished;
                    return Ok(());
                }

                self.buffer.write(&temp[..n]);
                continue;
            }

            // Pad if needed
            if bytes_read < bytes_per_packet {
                packet_data[bytes_read..].fill(0);
            }

            // Encode to RTP
            let rtp_packet = {
                let mut codec = self.rtp_codec.lock().await;
                codec.encode_audio(&packet_data)?
            };

            // Send packet
            self.send_packet(&rtp_packet).await?;

            // Pace the sending
            tokio::time::sleep(packet_duration).await;

            // Refill buffer in background
            if self.buffer.is_underrunning() {
                let mut temp = vec![0u8; bytes_per_packet * 4];
                if let Ok(n) = source.read(&mut temp) {
                    if n > 0 {
                        self.buffer.write(&temp[..n]);
                    }
                }
            }
        }
    }

    /// Send an RTP packet
    async fn send_packet(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.connection.send_rtp_audio(packet).await?;
        Ok(())
    }

    /// Pause streaming
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.cmd_tx.send(StreamerCommand::Pause).await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Resume streaming
    pub async fn resume(&self) -> Result<(), AirPlayError> {
        self.cmd_tx.send(StreamerCommand::Resume).await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Stop streaming
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.cmd_tx.send(StreamerCommand::Stop).await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Seek to position
    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.cmd_tx.send(StreamerCommand::Seek(position)).await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::source::SilenceSource;

    // Note: These tests require a mock connection manager
}
```

---

### 13.3 Module Entry Point

- [ ] **13.3.1** Create streaming module

**File:** `src/streaming/mod.rs`

```rust
//! Audio streaming

mod source;
mod pcm;

pub use source::{AudioSource, SliceSource, CallbackSource, SilenceSource};
pub use pcm::{PcmStreamer, StreamerState};
```

---

## Acceptance Criteria

- [ ] AudioSource trait defined with implementations
- [ ] PCM streamer encodes audio to RTP
- [ ] Buffer management prevents underruns
- [ ] Pause/resume works correctly
- [ ] Seek works for seekable sources
- [ ] All unit tests pass

---

## Notes

- Actual UDP sending will be implemented with connection
- Consider adding audio level metering
- May need to handle clock drift compensation
- Buffered mode would use larger packets
