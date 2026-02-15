//! PCM audio streaming to `AirPlay` devices

use super::ResamplingSource;
use super::source::AudioSource;
use crate::audio::aac_encoder::AacEncoder;
use crate::audio::{AudioFormat, AudioRingBuffer};
use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::protocol::ptp::{PtpTimestamp, SharedPtpClock};
use crate::protocol::rtp::{RtpCodec, TimeAnnouncePtp};

use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, mpsc};

/// RTP packet sender trait
#[async_trait]
pub trait RtpSender: Send + Sync {
    /// Send RTP audio packet
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError>;

    /// Send control packet (e.g. `TimeAnnouncePtp`)
    async fn send_control_packet(&self, packet: &[u8]) -> Result<(), AirPlayError>;

    /// Get shared PTP clock
    async fn ptp_clock(&self) -> Option<SharedPtpClock>;
}

#[async_trait]
impl RtpSender for ConnectionManager {
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.send_rtp_audio(packet).await
    }

    async fn send_control_packet(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.send_control_packet(packet).await
    }

    async fn ptp_clock(&self) -> Option<SharedPtpClock> {
        self.ptp_clock().await
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

        // Reusable buffer for samples to avoid allocations
        let mut samples_buffer = Vec::with_capacity(bytes_per_packet / 2);

        // Reusable buffer for encoding output to avoid allocations
        let mut encoding_buffer = vec![0u8; 4096];

        // RTP timestamp tracking
        let mut last_announce = Instant::now();

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
                        // Reset accumulated frames logic if seeking?
                        // Usually timestamp continues or jumps.
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
                continue;
            }

            // Pad if needed
            if bytes_read < bytes_per_packet {
                packet_data[bytes_read..].fill(0);
            }

            // Encode payload
            let encoded_payload: Cow<'_, [u8]> = {
                let codec_type = *self.codec_type.read().await;
                match codec_type {
                    AudioCodec::Alac => {
                        let mut encoder_guard = self.encoder.lock().await;
                        if let Some(encoder) = encoder_guard.as_mut() {
                            let input_format = alac_encoder::FormatDescription::pcm::<i16>(
                                f64::from(self.format.sample_rate.as_u32()),
                                u32::from(self.format.channels.channels()),
                            );

                            if encoding_buffer.len() < 4096 {
                                encoding_buffer.resize(4096, 0);
                            }

                            let size =
                                encoder.encode(&input_format, &packet_data, &mut encoding_buffer);
                            let safe_size = size.min(encoding_buffer.len());
                            Cow::Borrowed(&encoding_buffer[..safe_size])
                        } else {
                            Cow::Borrowed(&packet_data)
                        }
                    }
                    AudioCodec::Aac => {
                        let mut encoder_guard = self.encoder_aac.lock().await;
                        if let Some(encoder) = encoder_guard.as_mut() {
                            samples_buffer.clear();
                            samples_buffer.extend(
                                packet_data
                                    .chunks_exact(2)
                                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]])),
                            );

                            match encoder.encode(&samples_buffer) {
                                Ok(encoded) => {
                                    let mut payload = Vec::with_capacity(4 + encoded.len());
                                    payload.extend_from_slice(&[0x00, 0x10]);
                                    #[allow(clippy::cast_possible_truncation)]
                                    let size = encoded.len() as u16;
                                    let header = (size << 3) & 0xFFF8;
                                    payload.extend_from_slice(&header.to_be_bytes());
                                    payload.extend_from_slice(&encoded);
                                    Cow::Owned(payload)
                                }
                                Err(e) => {
                                    tracing::error!("AAC encoding error: {}", e);
                                    Cow::Borrowed(&packet_data)
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
            let rtp_timestamp_start = {
                let mut codec = self.rtp_codec.lock().await;
                // Capture timestamp before encoding
                let ts = codec.timestamp();
                codec
                    .encode_arbitrary_payload(&encoded_payload, &mut rtp_packet_buffer)
                    .map_err(|e| AirPlayError::RtpError {
                        message: e.to_string(),
                    })?;
                ts
            };

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

            // Send TimeAnnouncePtp if needed (every 1 second)
            if last_announce.elapsed() >= Duration::from_secs(1) {
                if let Some(clock) = self.connection.ptp_clock().await {
                    let ptp_time = PtpTimestamp::now();
                    // We use the start timestamp of the current packet as reference
                    // Convert accumulated frames to timestamp units?
                    // Actually we got `rtp_timestamp_start` from codec.
                    let rtp_ts = rtp_timestamp_start;

                    // We need to associate this RTP timestamp with a PTP time.
                    // Ideally, we know when this packet will be played.
                    // But for TimeAnnouncePtp, we just announce the mapping.
                    // "monotonic_ns is the sender's PTP timestamp, i.e. uptime."
                    // And "pkt with senderRtpTimestamp is the RTP 'clock' time with the NTP timestamp..."
                    // We can use current time as PTP time, and current RTP timestamp.
                    // But we must account for the fact that the packet we just sent corresponds to audio some time ago?
                    // No, `rtp_timestamp_start` is the timestamp of the packet we just sent.
                    // `PtpTimestamp::now()` is "now".
                    // The receiver uses this to sync RTP clock to PTP clock.
                    // If we send (now, rtp_ts), we are saying "audio at rtp_ts corresponds to time now".
                    // But if we just sent it, it will be played in the future (latency).
                    // The mapping should be accurate.
                    // If we generated the packet "now", then "now" is the capture time?
                    // Yes, we just read it from buffer.
                    // So (now, rtp_ts) is a valid anchor point.

                    let clock_id = clock.read().await.clock_id();
                    // ptp_timestamp needs to be u64 nanoseconds (monotonic usually)
                    // But PtpTimestamp::now() is wall clock.
                    // We decided to use PtpTimestamp::now() for now.
                    // Need to convert PtpTimestamp to u64 nanos.
                    // PtpTimestamp::to_nanos() returns i128.
                    #[allow(clippy::cast_sign_loss)]
                    #[allow(clippy::cast_possible_truncation)]
                    let ptp_nanos = ptp_time.to_nanos() as u64;

                    // Next timestamp? Maybe rtp_ts + sample_rate?
                    let next_rtp_ts = rtp_ts.wrapping_add(self.format.sample_rate.as_u32());

                    let announce = TimeAnnouncePtp::new(rtp_ts, ptp_nanos, next_rtp_ts, clock_id);

                    tracing::debug!(
                        "Sending TimeAnnouncePtp: RTP={}, PTP={}ns",
                        rtp_ts,
                        ptp_nanos
                    );
                    if let Err(e) = self
                        .connection
                        .send_control_packet(&announce.encode())
                        .await
                    {
                        tracing::warn!("Failed to send TimeAnnouncePtp: {}", e);
                    }

                    last_announce = Instant::now();
                }
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
