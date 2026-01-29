//! PCM audio streaming to `AirPlay` devices

#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]

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

use crate::audio::AudioCodec;

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
    /// ALAC encoder
    encoder: Mutex<Option<alac_encoder::AlacEncoder>>,
    /// Codec type
    codec_type: RwLock<AudioCodec>,
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
            encoder: Mutex::new(None),
            codec_type: RwLock::new(AudioCodec::Pcm),
        }
    }

    /// Set ChaCha20-Poly1305 encryption key
    pub async fn set_encryption_key(&self, key: [u8; 32]) {
        let mut codec = self.rtp_codec.lock().await;
        codec.set_chacha_encryption(key);
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

        tracing::debug!(
            "Filling buffer: capacity={}, high_watermark={}",
            self.buffer.capacity(),
            self.buffer.capacity() * 3 / 4
        );

        while !self.buffer.is_ready() {
            let n = source
                .read(&mut temp_buffer)
                .map_err(|e| AirPlayError::IoError {
                    message: "Failed to read from source".to_string(),
                    source: Some(Box::new(e)),
                })?;
            if n == 0 {
                tracing::debug!(
                    "Source EOF during buffer fill, available={}",
                    self.buffer.available()
                );
                break; // EOF
            }
            let written = self.buffer.write(&temp_buffer[..n]);
            tracing::trace!(
                "Buffer fill: read={}, written={}, available={}",
                n,
                written,
                self.buffer.available()
            );
        }

        tracing::debug!("Buffer filled: available={}", self.buffer.available());
        Ok(())
    }

    /// Main streaming loop
    #[allow(clippy::too_many_lines)]
    async fn streaming_loop<S: AudioSource>(&self, mut source: S) -> Result<(), AirPlayError> {
        let bytes_per_packet = Self::FRAMES_PER_PACKET * self.format.bytes_per_frame();
        let packet_duration = self.format.frames_to_duration(Self::FRAMES_PER_PACKET);

        tracing::debug!(
            "Starting streaming loop: bytes_per_packet={}, packet_duration={:?}",
            bytes_per_packet,
            packet_duration
        );

        let mut packet_data = vec![0u8; bytes_per_packet];
        let mut cmd_rx = self.cmd_rx.lock().await;

        // Use interval for precise timing
        let mut interval = tokio::time::interval(packet_duration);
        // The first tick completes immediately
        interval.tick().await;

        // Reusable buffer for refills
        let mut refill_buffer = vec![0u8; bytes_per_packet * 4];
        let mut packets_sent = 0u64;

        // Reusable buffer for RTP packet to avoid allocations
        let mut rtp_packet_buffer = Vec::with_capacity(bytes_per_packet + 64);

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
            tracing::trace!(
                "Read {} bytes from buffer, available={}",
                bytes_read,
                self.buffer.available()
            );

            if bytes_read == 0 {
                // Try to fill buffer
                let mut temp = vec![0u8; bytes_per_packet * 2];
                let n = source.read(&mut temp).map_err(|e| AirPlayError::IoError {
                    message: "Read failed".to_string(),
                    source: Some(Box::new(e)),
                })?;

                if n == 0 {
                    // EOF
                    tracing::debug!("Source EOF after {} packets sent", packets_sent);
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

            // Encode payload
            let encoded_payload = {
                let codec_type = *self.codec_type.read().await;
                if codec_type == AudioCodec::Alac {
                    let mut encoder_guard = self.encoder.lock().await;
                    if let Some(encoder) = encoder_guard.as_mut() {
                        // alac-encoder 0.3.0 expects byte slice of PCM data
                        // and a FormatDescription for that input
                        let input_format = alac_encoder::FormatDescription::pcm::<i16>(
                            self.format.sample_rate.as_u32() as f64,
                            self.format.channels.channels() as u32,
                        );

                        let mut out_buffer = vec![0u8; 4096];

                        let size = encoder.encode(&input_format, &packet_data, &mut out_buffer);
                        out_buffer[..size].to_vec()
                    } else {
                        packet_data.clone()
                    }
                } else {
                    packet_data.clone()
                }
            };

            // Encrypt and wrap in RTP
            rtp_packet_buffer.clear();
            {
                let mut codec = self.rtp_codec.lock().await;
                codec
                    .encode_arbitrary_payload(&encoded_payload, &mut rtp_packet_buffer)
                    .map_err(|e| AirPlayError::RtpError {
                        message: e.to_string(),
                    })?;
            }

            // Send packet
            self.send_packet(&rtp_packet_buffer).await?;
            packets_sent += 1;
            if packets_sent % 100 == 0 {
                tracing::debug!("Sent {} RTP packets", packets_sent);
            }

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
        tracing::trace!("Sending RTP packet: {} bytes", packet.len());
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

    /// Set codec to ALAC
    pub async fn use_alac(&self) {
        let format = alac_encoder::FormatDescription::alac(
            self.format.sample_rate.as_u32() as f64,
            Self::FRAMES_PER_PACKET as u32,
            self.format.channels.channels() as u32,
        );
        *self.encoder.lock().await = Some(alac_encoder::AlacEncoder::new(&format));
        *self.codec_type.write().await = AudioCodec::Alac;
    }

    /// Set codec to PCM (default)
    pub async fn use_pcm(&self) {
        *self.encoder.lock().await = None;
        *self.codec_type.write().await = AudioCodec::Pcm;
    }
}
