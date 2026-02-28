//! RTP packet receiver for audio data
//!
//! Handles incoming RTP packets on the audio UDP port,
//! decrypts them, and forwards to the jitter buffer.

use std::sync::Arc;

use aes::Aes128;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, KeyInit};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::protocol::rtp::{RtpDecodeError, RtpHeader};
use crate::receiver::session::StreamParameters;

/// Maximum UDP packet size
const MAX_PACKET_SIZE: usize = 2048;

/// Received and decrypted audio packet
#[derive(Debug, Clone)]
pub struct AudioPacket {
    /// RTP sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// SSRC
    pub ssrc: u32,
    /// Decrypted audio data
    pub audio_data: Vec<u8>,
    /// Reception time (for jitter calculation)
    pub received_at: std::time::Instant,
}

/// Errors from RTP reception
#[derive(Debug, thiserror::Error)]
pub enum RtpReceiveError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid RTP packet
    #[error("Invalid RTP packet: {0}")]
    InvalidPacket(#[from] RtpDecodeError),

    /// Wrong payload type
    #[error("Wrong payload type: {0:02x}")]
    WrongPayloadType(u8),

    /// Decryption failed
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    /// Channel closed
    #[error("Channel closed")]
    ChannelClosed,
}

/// Audio payload decryptor (AES-128-CBC)
pub struct AudioDecryptor {
    key: [u8; 16],
    iv: [u8; 16],
}

impl AudioDecryptor {
    /// Create a new audio decryptor
    #[must_use]
    pub fn new(key: [u8; 16], iv: [u8; 16]) -> Self {
        Self { key, iv }
    }

    /// Decrypt audio payload
    ///
    /// RAOP uses AES-128-CBC with the IV from SDP.
    /// Each packet uses the same IV (not chained between packets).
    ///
    /// # Errors
    /// Returns `RtpReceiveError` if decryption fails (though currently AES-128-CBC via `aes` crate
    /// doesn't typically fail if input is valid size).
    ///
    /// # Panics
    /// Panics if the chunk size in the loop is not 16 bytes, which shouldn't happen due to
    /// `chunks(block_size)`.
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, RtpReceiveError> {
        if encrypted.is_empty() {
            return Ok(Vec::new());
        }

        // AES-CBC works on 16-byte blocks
        // RAOP only encrypts complete blocks, leaving remainder unencrypted
        let block_size = 16;
        let encrypted_len = (encrypted.len() / block_size) * block_size;

        if encrypted_len == 0 {
            // Less than one block, no encryption
            return Ok(encrypted.to_vec());
        }

        let cipher = Aes128::new(GenericArray::from_slice(&self.key));

        let mut decrypted = Vec::with_capacity(encrypted.len());

        // Decrypt in CBC mode
        let mut prev_block = self.iv;

        for chunk in encrypted[..encrypted_len].chunks(block_size) {
            let mut block = GenericArray::clone_from_slice(chunk);

            // Save ciphertext for next XOR
            let ciphertext: [u8; 16] = chunk
                .try_into()
                .map_err(|_| RtpReceiveError::DecryptionFailed("Invalid chunk size".to_string()))?;

            // Decrypt block
            cipher.decrypt_block(&mut block);

            // XOR with previous ciphertext (or IV for first block)
            for (b, p) in block.iter_mut().zip(prev_block.iter()) {
                *b ^= *p;
            }

            decrypted.extend_from_slice(&block);
            prev_block = ciphertext;
        }

        // Append unencrypted remainder
        if encrypted_len < encrypted.len() {
            decrypted.extend_from_slice(&encrypted[encrypted_len..]);
        }

        Ok(decrypted)
    }
}

/// RTP audio receiver
pub struct RtpAudioReceiver {
    socket: Arc<UdpSocket>,
    #[allow(dead_code)] // Keep for future reference or debugging
    stream_params: StreamParameters,
    packet_tx: mpsc::Sender<AudioPacket>,
    decryptor: Option<AudioDecryptor>,
}

impl RtpAudioReceiver {
    /// Create a new RTP audio receiver
    #[must_use]
    pub fn new(
        socket: Arc<UdpSocket>,
        stream_params: StreamParameters,
        packet_tx: mpsc::Sender<AudioPacket>,
    ) -> Self {
        let decryptor = if let (Some(key), Some(iv)) = (stream_params.aes_key, stream_params.aes_iv)
        {
            Some(AudioDecryptor::new(key, iv))
        } else {
            None
        };

        Self {
            socket,
            stream_params,
            packet_tx,
            decryptor,
        }
    }

    /// Run the receive loop
    ///
    /// # Errors
    /// Returns `RtpReceiveError` if socket access fails.
    pub async fn run(self) -> Result<(), RtpReceiveError> {
        let mut buf = [0u8; MAX_PACKET_SIZE];

        loop {
            let (len, _src) = self.socket.recv_from(&mut buf).await?;

            if len < RtpHeader::SIZE {
                // Too short for RTP header
                continue;
            }

            match self.process_packet(&buf[..len]).await {
                Ok(()) => {}
                Err(RtpReceiveError::ChannelClosed) => {
                    tracing::debug!("Audio channel closed, stopping receiver");
                    break;
                }
                Err(e) => {
                    tracing::warn!("RTP packet error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Process a single RTP packet
    async fn process_packet(&self, data: &[u8]) -> Result<(), RtpReceiveError> {
        // Parse RTP header
        let header = RtpHeader::decode(data)?;

        // Check payload type
        if !matches!(
            header.payload_type,
            crate::protocol::rtp::PayloadType::AudioRealtime
                | crate::protocol::rtp::PayloadType::AudioBuffered
        ) {
            return Err(RtpReceiveError::WrongPayloadType(header.payload_type as u8));
        }

        // Extract payload (after header size)
        let payload = &data[RtpHeader::SIZE..];

        // Decrypt if encryption is enabled
        let audio_data = if let Some(ref decryptor) = self.decryptor {
            decryptor.decrypt(payload)?
        } else {
            payload.to_vec()
        };

        // Create audio packet
        let packet = AudioPacket {
            sequence: header.sequence,
            timestamp: header.timestamp,
            ssrc: header.ssrc,
            audio_data,
            received_at: std::time::Instant::now(),
        };

        // Send to jitter buffer
        self.packet_tx
            .send(packet)
            .await
            .map_err(|_| RtpReceiveError::ChannelClosed)?;

        Ok(())
    }
}
