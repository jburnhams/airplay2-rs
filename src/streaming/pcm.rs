//! PCM audio streaming to `AirPlay` devices

use super::source::AudioSource;
use crate::audio::{AudioFormat, AudioRingBuffer};
use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::protocol::rtp::RtpCodec;

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};

/// RTP packet sender trait
#[async_trait]
pub trait RtpSender: Send + Sync {
    /// Send RTP audio packet
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError>;
}

#[async_trait]
impl RtpSender for ConnectionManager {
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.send_rtp_audio(packet).await
    }
}

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
    #[allow(dead_code)]
    Error,
}

/// PCM audio streamer
pub struct PcmStreamer {
    /// Connection manager
    connection: Arc<dyn RtpSender>,
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
    /// Frames per RTP packet (standard `AirPlay`)
    pub const FRAMES_PER_PACKET: usize = 352;

    /// Create a new PCM streamer
    #[must_use]
    pub fn new<C: RtpSender + 'static>(connection: Arc<C>, format: AudioFormat) -> Self {
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
    ///
    /// # Errors
    ///
    /// Returns error if streaming fails
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
        self.fill_buffer(&mut source)?;

        *self.state.write().await = StreamerState::Streaming;

        // Start streaming loop
        self.streaming_loop(source).await
    }

    /// Fill the audio buffer from source
    fn fill_buffer<S: AudioSource>(&self, source: &mut S) -> Result<(), AirPlayError> {
        let bytes_per_packet = Self::FRAMES_PER_PACKET * self.format.bytes_per_frame();
        let mut temp_buffer = vec![0u8; bytes_per_packet * 4];

        while !self.buffer.is_ready() {
            let n = source
                .read(&mut temp_buffer)
                .map_err(|e| AirPlayError::IoError {
                    message: "Failed to read from source".to_string(),
                    source: Some(Box::new(e)),
                })?;
            if n == 0 {
                break; // EOF
            }
            self.buffer.write(&temp_buffer[..n]);
        }

        Ok(())
    }

    /// Main streaming loop
    async fn streaming_loop<S: AudioSource>(&self, mut source: S) -> Result<(), AirPlayError> {
        let bytes_per_packet = Self::FRAMES_PER_PACKET * self.format.bytes_per_frame();
        let packet_duration = self.format.frames_to_duration(Self::FRAMES_PER_PACKET);

        let mut packet_data = vec![0u8; bytes_per_packet];
        let mut cmd_rx = self.cmd_rx.lock().await;

        // Use interval for precise timing
        let mut interval = tokio::time::interval(packet_duration);
        // The first tick completes immediately
        interval.tick().await;

        // Reusable buffer for refills
        let mut refill_buffer = vec![0u8; bytes_per_packet * 4];

        loop {
            // Wait for next tick
            interval.tick().await;

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
                        source.seek(pos).map_err(|e| AirPlayError::IoError {
                            message: "Seek failed".to_string(),
                            source: Some(Box::new(e)),
                        })?;
                        self.buffer.clear();
                        self.fill_buffer(&mut source)?;
                    }
                }
                _ => {}
            }

            // Read from buffer
            let bytes_read = self.buffer.read(&mut packet_data);

            if bytes_read == 0 {
                // Try to fill buffer
                let mut temp = vec![0u8; bytes_per_packet * 2];
                let n = source.read(&mut temp).map_err(|e| AirPlayError::IoError {
                    message: "Read failed".to_string(),
                    source: Some(Box::new(e)),
                })?;

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
                codec
                    .encode_audio(&packet_data)
                    .map_err(|e| AirPlayError::RtpError {
                        message: e.to_string(),
                    })?
            };

            // Send packet
            self.send_packet(&rtp_packet).await?;

            // Refill buffer in background
            if self.buffer.is_underrunning() {
                if let Ok(n) = source.read(&mut refill_buffer) {
                    if n > 0 {
                        self.buffer.write(&refill_buffer[..n]);
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
    ///
    /// # Errors
    ///
    /// Returns error if streamer is not running
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.cmd_tx
            .send(StreamerCommand::Pause)
            .await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Resume streaming
    ///
    /// # Errors
    ///
    /// Returns error if streamer is not running
    pub async fn resume(&self) -> Result<(), AirPlayError> {
        self.cmd_tx
            .send(StreamerCommand::Resume)
            .await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Stop streaming
    ///
    /// # Errors
    ///
    /// Returns error if streamer is not running
    pub async fn stop(&self) -> Result<(), AirPlayError> {
        self.cmd_tx
            .send(StreamerCommand::Stop)
            .await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }

    /// Seek to position
    ///
    /// # Errors
    ///
    /// Returns error if streamer is not running
    pub async fn seek(&self, position: Duration) -> Result<(), AirPlayError> {
        self.cmd_tx
            .send(StreamerCommand::Seek(position))
            .await
            .map_err(|_| AirPlayError::InvalidState {
                message: "Streamer not running".to_string(),
                current_state: "unknown".to_string(),
            })
    }
}
