//! PCM audio streaming to `AirPlay` devices

use super::ResamplingSource;
use super::source::AudioSource;
use crate::audio::aac_encoder::AacEncoder;
use crate::audio::{AudioFormat, AudioRingBuffer};
use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::protocol::rtp::RtpCodec;

use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};

/// RTP packet sender trait
#[async_trait]
pub trait RtpSender: Send + Sync {
    /// Send RTP audio packet
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError>;

    /// Send PTP Time Announce control packet
    async fn send_time_announce(
        &self,
        rtp_timestamp: u32,
        sample_rate: u32,
    ) -> Result<(), AirPlayError>;
}

#[async_trait]
impl RtpSender for ConnectionManager {
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.send_rtp_audio(packet).await
    }

    async fn send_time_announce(
        &self,
        rtp_timestamp: u32,
        sample_rate: u32,
    ) -> Result<(), AirPlayError> {
        self.send_time_announce(rtp_timestamp, sample_rate).await
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
    /// AAC encoder
    encoder_aac: Mutex<Option<AacEncoder>>,
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
            encoder_aac: Mutex::new(None),
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
        if source.format() == self.format {
            *self.state.write().await = StreamerState::Buffering;

            // Fill buffer initially
            self.fill_buffer(&mut source)?;

            *self.state.write().await = StreamerState::Streaming;

            // Start streaming loop
            self.streaming_loop(source).await
        } else {
            tracing::info!(
                "Source format ({:?}) differs from output format ({:?}). Enabling resampling.",
                source.format(),
                self.format
            );

            let mut resampled =
                ResamplingSource::new(source, self.format).map_err(|e| AirPlayError::IoError {
                    message: format!("Failed to create resampler: {e}"),
                    source: Some(Box::new(e)),
                })?;

            *self.state.write().await = StreamerState::Buffering;

            // Fill buffer initially
            self.fill_buffer(&mut resampled)?;

            *self.state.write().await = StreamerState::Streaming;

            // Start streaming loop
            self.streaming_loop(resampled).await
        }
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
    // Complexity is necessary for the main streaming logic
    #[allow(clippy::too_many_lines)]
    async fn streaming_loop<S: AudioSource>(&self, mut source: S) -> Result<(), AirPlayError> {
        let codec_type = *self.codec_type.read().await;
        let frames_per_packet = match codec_type {
            AudioCodec::Aac => 1024,
            _ => Self::FRAMES_PER_PACKET,
        };

        // Update RTP codec with correct frame size
        {
            #[allow(clippy::cast_possible_truncation, reason = "Frame count fits in u32")]
            let frames = frames_per_packet as u32;
            self.rtp_codec.lock().await.set_frames_per_packet(frames);
        }

        let bytes_per_packet = frames_per_packet * self.format.bytes_per_frame();
        let packet_duration = self.format.frames_to_duration(frames_per_packet);

        tracing::debug!(
            "Starting streaming loop: bytes_per_packet={}, packet_duration={:?}",
            bytes_per_packet,
            packet_duration
        );

        let mut packet_data = vec![0u8; bytes_per_packet];
        let mut cmd_rx = self.cmd_rx.lock().await;

        // Use interval for precise timing of audio packets
        let mut audio_interval = tokio::time::interval(packet_duration);
        audio_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick completes immediately
        audio_interval.tick().await;

        // Use a separate interval for periodic time announcements (every 1 second)
        let mut announce_interval = tokio::time::interval(Duration::from_secs(1));
        announce_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Reusable buffer for refills
        let mut refill_buffer = vec![0u8; bytes_per_packet * 4];
        let mut packets_sent = 0u64;

        // Reusable buffer for RTP packet to avoid allocations
        let mut rtp_packet_buffer = Vec::with_capacity(bytes_per_packet + 64);

        // Reusable buffer for samples to avoid allocations
        let mut samples_buffer = Vec::with_capacity(bytes_per_packet / 2);

        // Reusable buffer for encoding output to avoid allocations
        let mut encoding_buffer = vec![0u8; 4096];

        loop {
            tokio::select! {
                // Audio packet processing
                _ = audio_interval.tick() => {
                    // Read from buffer
                    let bytes_read = self.buffer.read(&mut packet_data);
                    tracing::trace!(
                        "Read {} bytes from buffer, available={}",
                        bytes_read,
                        self.buffer.available()
                    );

                    if bytes_read == 0 {
                        // Try to fill buffer
                        let n = source
                            .read(&mut refill_buffer)
                            .map_err(|e| AirPlayError::IoError {
                                message: "Read failed".to_string(),
                                source: Some(Box::new(e)),
                            })?;

                        if n == 0 {
                            // EOF
                            tracing::debug!("Source EOF after {} packets sent", packets_sent);
                            *self.state.write().await = StreamerState::Finished;
                            return Ok(());
                        }

                        self.buffer.write(&refill_buffer[..n]);
                        // Don't send a packet this tick if we just refilled (or should we?)
                        // If we just refilled, we can try to read again immediately?
                        // For simplicity in select! loop, we'll just wait for next tick or let the loop continue?
                        // Actually, if we return from select!, the loop continues.
                        // But we consumed the tick.
                        // Let's retry reading immediately?
                        // Better: just continue the loop, next tick will happen or if we are behind it will fire immediately.
                        continue;
                    }

                    // Pad if needed
                    if bytes_read < bytes_per_packet {
                        packet_data[bytes_read..].fill(0);
                    }

                    // Encode payload
                    let encoded_payload: Cow<'_, [u8]> = {
                        match codec_type {
                            AudioCodec::Alac => {
                                let mut encoder_guard = self.encoder.lock().await;
                                if let Some(encoder) = encoder_guard.as_mut() {
                                    // alac-encoder 0.3.0 expects byte slice of PCM data
                                    // and a FormatDescription for that input
                                    let input_format = alac_encoder::FormatDescription::pcm::<i16>(
                                        f64::from(self.format.sample_rate.as_u32()),
                                        u32::from(self.format.channels.channels()),
                                    );

                                    // Ensure encoding buffer has enough capacity
                                    if encoding_buffer.len() < 4096 {
                                        encoding_buffer.resize(4096, 0);
                                    }

                                    let size =
                                        encoder.encode(&input_format, &packet_data, &mut encoding_buffer);
                                    // Safety: clamp size to buffer length to prevent panic if encoder returns a size
                                    // larger than the buffer (which would have been safe with the original .truncate(size))
                                    let safe_size = size.min(encoding_buffer.len());
                                    Cow::Borrowed(&encoding_buffer[..safe_size])
                                } else {
                                    Cow::Borrowed(&packet_data)
                                }
                            }
                            AudioCodec::Aac => {
                                let mut encoder_guard = self.encoder_aac.lock().await;
                                if let Some(encoder) = encoder_guard.as_mut() {
                                    // Convert bytes to i16 (Little Endian)
                                    // We assume input is always I16 Little Endian (standard AirPlay/PCM)
                                    samples_buffer.clear();
                                    samples_buffer.extend(
                                        packet_data
                                            .chunks_exact(2)
                                            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]])),
                                    );

                                    match encoder.encode(&samples_buffer) {
                                        Ok(encoded) => {
                                            // Add AU Header Section for mpeg4-generic (RFC 3640)
                                            // AU-headers-length: 16 bits (0x0010) = 16
                                            // AU-header: size (13 bits) | index (3 bits)
                                            let mut payload = Vec::with_capacity(4 + encoded.len());
                                            payload.extend_from_slice(&[0x00, 0x10]);

                                            // AAC frames are small enough to fit in u16
                                            #[allow(clippy::cast_possible_truncation)]
                                            let size = encoded.len() as u16;
                                            let header = (size << 3) & 0xFFF8;
                                            payload.extend_from_slice(&header.to_be_bytes());

                                            payload.extend_from_slice(&encoded);
                                            Cow::Owned(payload)
                                        }
                                        Err(e) => {
                                            tracing::error!("AAC encoding error: {}", e);
                                            Cow::Borrowed(&packet_data) // Fallback (will likely sound like static)
                                        }
                                    }
                                } else {
                                    Cow::Borrowed(&packet_data)
                                }
                            }
                            _ => Cow::Borrowed(&packet_data),
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
                    if packets_sent == 1 {
                        tracing::info!(
                            "First RTP audio packet sent ({} bytes)",
                            rtp_packet_buffer.len()
                        );
                    }
                    if packets_sent % 100 == 0 {
                        tracing::info!("Sent {} RTP packets", packets_sent);
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

                // Time Announcement
                _ = announce_interval.tick() => {
                     // Get current RTP timestamp from codec
                    let rtp_ts = self.rtp_codec.lock().await.timestamp();
                    if let Err(e) = self
                        .connection
                        .send_time_announce(rtp_ts, self.format.sample_rate.as_u32())
                        .await
                    {
                        tracing::warn!("Failed to send Time Announce: {}", e);
                    }
                }

                // Command processing
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(StreamerCommand::Pause) => {
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
                            // Reset intervals to avoid burst?
                            // audio_interval.reset();
                        }
                        Some(StreamerCommand::Stop) => {
                            *self.state.write().await = StreamerState::Idle;
                            return Ok(());
                        }
                        Some(StreamerCommand::Seek(pos)) => {
                            if source.is_seekable() {
                                source.seek(pos).map_err(|e| AirPlayError::IoError {
                                    message: "Seek failed".to_string(),
                                    source: Some(Box::new(e)),
                                })?;
                                self.buffer.clear();
                                self.fill_buffer(&mut source)?;
                            }
                        }
                        None => {
                            // Channel closed
                            tracing::debug!("Command channel closed, stopping streamer");
                            *self.state.write().await = StreamerState::Idle;
                            return Ok(());
                        }
                        _ => {}
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
        // FRAMES_PER_PACKET (352) fits in u32
        #[allow(clippy::cast_possible_truncation)]
        let format = alac_encoder::FormatDescription::alac(
            f64::from(self.format.sample_rate.as_u32()),
            Self::FRAMES_PER_PACKET as u32,
            u32::from(self.format.channels.channels()),
        );
        *self.encoder.lock().await = Some(alac_encoder::AlacEncoder::new(&format));
        *self.encoder_aac.lock().await = None;
        *self.codec_type.write().await = AudioCodec::Alac;
    }

    /// Set codec to AAC
    ///
    /// # Panics
    ///
    /// Panics if the AAC encoder cannot be initialized (e.g. invalid parameters).
    pub async fn use_aac(&self, bitrate: u32) {
        // Standard AAC-LC: 44100Hz, Stereo
        let encoder = AacEncoder::new(
            self.format.sample_rate.as_u32(),
            u32::from(self.format.channels.channels()),
            bitrate,
        )
        .expect("Failed to initialize AAC encoder");

        *self.encoder_aac.lock().await = Some(encoder);
        *self.encoder.lock().await = None;
        *self.codec_type.write().await = AudioCodec::Aac;
    }

    /// Set codec to PCM (default)
    pub async fn use_pcm(&self) {
        *self.encoder.lock().await = None;
        *self.encoder_aac.lock().await = None;
        *self.codec_type.write().await = AudioCodec::Pcm;
    }
}
