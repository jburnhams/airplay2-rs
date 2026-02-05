//! RAOP audio streaming coordinator

use crate::protocol::raop::RaopSessionKeys;
use crate::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer};
use crate::protocol::rtp::raop::{RaopAudioPacket, SyncPacket};
use crate::protocol::rtp::raop_timing::TimingSync;
use aes::Aes128;
use aes::cipher::KeyInit;
use aes::cipher::generic_array::GenericArray;
use std::time::{Duration, Instant};

/// RAOP streaming configuration
#[derive(Debug, Clone)]
pub struct RaopStreamConfig {
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Samples per packet (352 for ALAC)
    pub samples_per_packet: u32,
    /// SSRC for RTP packets
    pub ssrc: u32,
    /// Enable retransmission buffer
    pub enable_retransmit: bool,
}

impl Default for RaopStreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            samples_per_packet: 352,
            ssrc: rand::random(),
            enable_retransmit: true,
        }
    }
}

/// RAOP audio streamer
pub struct RaopStreamer {
    /// Configuration
    config: RaopStreamConfig,
    /// Current sequence number
    sequence: u16,
    /// Current RTP timestamp
    timestamp: u32,
    /// Session encryption keys
    keys: RaopSessionKeys,
    /// Pre-computed block cipher for key reuse
    aes_cipher: Aes128,
    /// Packet buffer for retransmission
    buffer: PacketBuffer,
    /// Timing synchronization
    timing: TimingSync,
    /// Is first packet after start/flush
    is_first_packet: bool,
    /// Last sync packet sent
    last_sync: Instant,
    /// Last timing request sent
    last_timing: Instant,
}

impl RaopStreamer {
    /// Timing request interval
    const TIMING_INTERVAL: Duration = Duration::from_secs(3);

    /// Sync packet interval
    const SYNC_INTERVAL: Duration = Duration::from_millis(1000);

    /// Create new streamer
    #[must_use]
    pub fn new(keys: RaopSessionKeys, config: RaopStreamConfig) -> Self {
        let key_generic = GenericArray::from_slice(keys.aes_key());
        let aes_cipher = Aes128::new(key_generic);

        Self {
            config,
            sequence: 0,
            timestamp: 0,
            keys,
            aes_cipher,
            buffer: PacketBuffer::new(PacketBuffer::DEFAULT_SIZE),
            timing: TimingSync::new(),
            is_first_packet: true,
            last_sync: Instant::now(),
            last_timing: Instant::now(),
        }
    }

    /// Get current sequence number
    #[must_use]
    pub fn sequence(&self) -> u16 {
        self.sequence
    }

    /// Get current timestamp
    #[must_use]
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Encode audio frame to RTP packet
    ///
    /// Audio should be encoded ALAC data (or raw PCM depending on codec)
    pub fn encode_frame(&mut self, audio_data: &[u8]) -> Vec<u8> {
        // Pre-allocate buffer with exact size
        let mut encoded = Vec::with_capacity(RaopAudioPacket::HEADER_SIZE + audio_data.len());

        // Write header directly
        RaopAudioPacket::write_header(
            &mut encoded,
            self.is_first_packet,
            self.sequence,
            self.timestamp,
            self.config.ssrc,
        );

        if self.is_first_packet {
            self.is_first_packet = false;
        }

        // Append audio data
        encoded.extend_from_slice(audio_data);

        // Encrypt payload in place
        // The payload starts after HEADER_SIZE
        self.encrypt_audio_in_place(&mut encoded[RaopAudioPacket::HEADER_SIZE..]);

        // Buffer for retransmission
        if self.config.enable_retransmit {
            self.buffer.push(BufferedPacket {
                sequence: self.sequence,
                timestamp: self.timestamp,
                data: encoded.clone(),
            });
        }

        // Update state
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(self.config.samples_per_packet);

        encoded
    }

    fn encrypt_audio_in_place(&self, data: &mut [u8]) {
        use crate::protocol::crypto::Aes128Ctr;

        let mut cipher = Aes128Ctr::new_with_cipher(&self.aes_cipher, self.keys.aes_iv())
            .expect("invalid AES keys");

        // AES-CTR encryption in place
        cipher.apply_keystream(data);
    }

    /// Handle retransmit request
    #[must_use]
    pub fn handle_retransmit(&self, seq_start: u16, count: u16) -> Vec<Vec<u8>> {
        self.buffer
            .get_range(seq_start, count)
            .map(|p| {
                // Wrap in retransmit response header
                let mut response = Vec::with_capacity(4 + p.data.len());
                response.push(0x80);
                response.push(0xD6); // PT=0x56 (retransmit response)
                response.extend_from_slice(&p.sequence.to_be_bytes());
                response.extend_from_slice(&p.data[4..]); // Skip original header
                response
            })
            .collect()
    }

    /// Check if sync packet should be sent
    #[must_use]
    pub fn should_send_sync(&self) -> bool {
        self.last_sync.elapsed() >= Self::SYNC_INTERVAL
    }

    /// Create sync packet
    pub fn create_sync_packet(&mut self) -> Vec<u8> {
        let ntp_time = crate::protocol::rtp::NtpTimestamp::now();
        let packet = SyncPacket::new(
            self.timestamp,
            ntp_time,
            self.timestamp.wrapping_add(self.config.samples_per_packet),
            false,
        );
        self.last_sync = Instant::now();
        packet.encode()
    }

    /// Check if timing request should be sent
    #[must_use]
    pub fn should_send_timing(&self) -> bool {
        self.last_timing.elapsed() >= Self::TIMING_INTERVAL
    }

    /// Create timing request
    pub fn create_timing_request(&mut self) -> Vec<u8> {
        self.last_timing = Instant::now();
        self.timing.create_request()
    }

    /// Process timing response
    ///
    /// # Errors
    ///
    /// Returns error string if response invalid (legacy reasons, should probably be Result<(), Error>)
    pub fn process_timing_response(&mut self, data: &[u8]) -> Result<(), String> {
        self.timing
            .process_response(data)
            .map_err(|e| e.to_string())
    }

    /// Flush and prepare for new playback
    pub fn flush(&mut self) {
        self.is_first_packet = true;
        self.buffer.clear();
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.sequence = 0;
        self.timestamp = 0;
        self.is_first_packet = true;
        self.buffer.clear();
        self.timing = TimingSync::new();
    }
}
