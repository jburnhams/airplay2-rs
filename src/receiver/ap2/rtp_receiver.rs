//! RTP Packet Receiver
//!
//! Receives RTP packets on the allocated UDP port and processes them.

use super::rtp_decryptor::{Ap2RtpDecryptor, AudioDecoder, DecryptionError};
use crate::protocol::rtp::RtpPacket;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Received audio frame containing decoded PCM samples
#[derive(Debug)]
pub struct AudioFrame {
    /// RTP sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// Decoded PCM samples (interleaved stereo)
    pub samples: Vec<i16>,
    /// Receive time (for jitter calculation)
    pub receive_time: std::time::Instant,
}

/// RTP receiver configuration
#[derive(Debug, Clone)]
pub struct RtpReceiverConfig {
    /// UDP port to listen on
    pub port: u16,
    /// Decryption key
    pub key: [u8; 32],
    /// Audio sample rate
    pub sample_rate: u32,
    /// Audio channels
    pub channels: u8,
    /// Bits per sample
    pub bits_per_sample: u8,
    /// Codec type (96=ALAC, 100=PCM, etc.)
    pub codec_type: u8,
}

/// RTP receiver
pub struct RtpReceiver {
    #[allow(dead_code)]
    config: RtpReceiverConfig,
    decryptor: Ap2RtpDecryptor,
    decoder: Box<dyn AudioDecoder>,
    /// Channel to send decoded frames
    frame_tx: mpsc::Sender<AudioFrame>,
    /// Statistics
    stats: ReceiverStats,
}

/// Statistics collected by the receiver
#[derive(Debug, Default)]
pub struct ReceiverStats {
    /// Total packets received
    pub packets_received: u64,
    /// Total packets successfully decrypted
    pub packets_decrypted: u64,
    /// Total packets that failed decryption
    pub packets_failed: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Total audio samples decoded
    pub samples_decoded: u64,
    /// Last received sequence number
    pub last_sequence: u16,
    /// Detected sequence gaps
    pub sequence_gaps: u64,
}

impl RtpReceiver {
    /// Create a new RTP receiver
    ///
    /// # Errors
    /// Returns `ReceiverError` if codec is unsupported or initialization fails.
    pub fn new(
        config: RtpReceiverConfig,
        frame_tx: mpsc::Sender<AudioFrame>,
    ) -> Result<Self, ReceiverError> {
        let decryptor = Ap2RtpDecryptor::new(config.key);

        // Create appropriate decoder based on codec type
        let decoder: Box<dyn AudioDecoder> = match config.codec_type {
            100 => Box::new(super::rtp_decryptor::PcmDecoder::new(
                config.sample_rate,
                config.channels,
                config.bits_per_sample,
            )),
            96 => {
                // ALAC - would need magic cookie from SETUP
                return Err(ReceiverError::UnsupportedCodec(config.codec_type));
            }
            _ => return Err(ReceiverError::UnsupportedCodec(config.codec_type)),
        };

        Ok(Self {
            config,
            decryptor,
            decoder,
            frame_tx,
            stats: ReceiverStats::default(),
        })
    }

    /// Process a received UDP packet
    ///
    /// # Errors
    /// Returns `ReceiverError` if packet processing fails (parse, decrypt, decode, send).
    pub fn process_packet(&mut self, data: &[u8]) -> Result<(), ReceiverError> {
        self.stats.packets_received += 1;
        self.stats.bytes_received += data.len() as u64;

        // Parse RTP header
        let packet =
            RtpPacket::decode(data).map_err(|e| ReceiverError::ParseError(e.to_string()))?;

        // Check for sequence gaps
        // We only flag a gap if the packet is "future" relative to expected.
        // If packet.sequence < expected (accounting for wrap), it's a late packet (reordering).
        // If packet.sequence > expected, we missed some packets.
        // Logic: (packet - expected) as i16.
        // If diff == 0: exact match.
        // If diff > 0 (small): gap.
        // If diff < 0 (small negative or large positive): late packet.

        if self.stats.packets_received > 0 {
            let expected_seq = self.stats.last_sequence.wrapping_add(1);
            #[allow(
                clippy::cast_possible_wrap,
                reason = "RTP sequence differences fit in i16"
            )]
            let diff = packet.header.sequence.wrapping_sub(expected_seq) as i16;

            match diff.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    // Gap detected (future packet)
                    self.stats.sequence_gaps += 1;
                    warn!(
                        "Sequence gap: expected {}, got {} (missed {} packets)",
                        expected_seq, packet.header.sequence, diff
                    );
                    // Update last_sequence to catch up
                    self.stats.last_sequence = packet.header.sequence;
                }
                std::cmp::Ordering::Less => {
                    // Late packet (reordered) - do not update last_sequence
                    // Just log if needed
                    // debug!("Late packet: expected {}, got {}", expected_seq, packet.header.sequence);
                }
                std::cmp::Ordering::Equal => {
                    // Exact match
                    self.stats.last_sequence = packet.header.sequence;
                }
            }
        } else {
            // First packet
            self.stats.last_sequence = packet.header.sequence;
        }

        // Decrypt payload
        let decrypted = self.decryptor.decrypt(&packet).map_err(|e| {
            self.stats.packets_failed += 1;
            ReceiverError::DecryptError(e)
        })?;

        self.stats.packets_decrypted += 1;

        // Decode audio
        let samples = self
            .decoder
            .decode(&decrypted)
            .map_err(|e| ReceiverError::DecodeError(e.to_string()))?;

        self.stats.samples_decoded += samples.len() as u64;

        // Create frame
        let frame = AudioFrame {
            sequence: packet.header.sequence,
            timestamp: packet.header.timestamp,
            samples,
            receive_time: std::time::Instant::now(),
        };

        // Send to jitter buffer
        self.frame_tx
            .try_send(frame)
            .map_err(|_| ReceiverError::ChannelFull)?;

        Ok(())
    }

    /// Get receiver statistics
    #[must_use]
    pub fn stats(&self) -> &ReceiverStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ReceiverStats::default();
    }
}

/// Errors occurring during receiver operation
#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    /// Failed to parse RTP packet
    #[error("Failed to parse RTP packet: {0}")]
    ParseError(String),

    /// Decryption failed
    #[error("Decryption failed: {0}")]
    DecryptError(#[from] DecryptionError),

    /// Audio decoding error
    #[error("Audio decode error: {0}")]
    DecodeError(String),

    /// Unsupported codec type
    #[error("Unsupported codec type: {0}")]
    UnsupportedCodec(u8),

    /// Frame channel is full (jitter buffer overflow?)
    #[error("Frame channel full")]
    ChannelFull,

    /// Underlying IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Async receiver task loop
///
/// Reads packets from UDP socket and processes them.
///
/// # Errors
/// Returns `ReceiverError` if critical error occurs (though loop handles most internally).
pub async fn run_receiver(
    socket: UdpSocket,
    mut receiver: RtpReceiver,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), ReceiverError> {
    let mut buf = [0u8; 2048];

    loop {
        tokio::select! {
            result = socket.recv(&mut buf) => {
                match result {
                    Ok(len) => {
                        if let Err(e) = receiver.process_packet(&buf[..len]) {
                            warn!("Packet processing error: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Socket receive error: {}", e);
                    }
                }
            }
            _ = shutdown.recv() => {
                info!("RTP receiver shutting down");
                break;
            }
        }
    }

    Ok(())
}
