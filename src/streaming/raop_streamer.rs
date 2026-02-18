//! RAOP audio streaming coordinator

use std::time::{Duration, Instant};

use bytes::{BufMut, Bytes, BytesMut};

use crate::protocol::crypto::Aes128Ctr;
use crate::protocol::raop::RaopSessionKeys;
use crate::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer};
use crate::protocol::rtp::raop::{RaopAudioPacket, SyncPacket};
use crate::protocol::rtp::raop_timing::TimingSync;

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
    /// AES-CTR stream cipher
    cipher: Aes128Ctr,
    /// Packet buffer for retransmission
    buffer: PacketBuffer,
    /// Encode buffer to avoid repeated allocations
    encode_buffer: BytesMut,
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
    ///
    /// # Panics
    ///
    /// Panics if the session keys have invalid length (must be 16 bytes for key and IV).
    #[must_use]
    pub fn new(keys: &RaopSessionKeys, config: RaopStreamConfig) -> Self {
        // Initialize AES-CTR cipher with session keys
        // We use expect() here because keys are guaranteed to be correct length
        // by RaopSessionKeys::generate() or parsing logic
        let cipher =
            Aes128Ctr::new(keys.aes_key(), keys.aes_iv()).expect("Invalid session keys length");

        Self {
            config,
            sequence: 0,
            timestamp: 0,
            cipher,
            buffer: PacketBuffer::new(PacketBuffer::DEFAULT_SIZE),
            encode_buffer: BytesMut::with_capacity(4096),
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
    pub fn encode_frame(&mut self, audio_data: &[u8]) -> Bytes {
        let packet_size = RaopAudioPacket::HEADER_SIZE + audio_data.len();
        self.encode_buffer.reserve(packet_size);

        // Write header directly
        RaopAudioPacket::write_header(
            &mut self.encode_buffer,
            self.is_first_packet,
            self.sequence,
            self.timestamp,
            self.config.ssrc,
        );

        if self.is_first_packet {
            self.is_first_packet = false;
        }

        // Append audio data
        self.encode_buffer.put_slice(audio_data);

        // Encrypt payload in place
        // The payload starts after HEADER_SIZE
        // Access the just-written part of the buffer
        let len = self.encode_buffer.len();
        let payload_start = len - audio_data.len();

        {
            let data = &mut self.encode_buffer[payload_start..];

            // Seek to the correct keystream offset based on RTP timestamp
            // AirPlay 2 uses 16-bit stereo PCM (4 bytes/sample) for timing
            let offset = u64::from(self.timestamp) * 4;
            self.cipher.seek(offset);
            self.cipher.apply_keystream(data);
        }

        // Extract the packet as Bytes
        // split() returns a new BytesMut containing [0, len), leaving self empty but with capacity
        let encoded_bytes = self.encode_buffer.split().freeze();

        // Buffer for retransmission
        if self.config.enable_retransmit {
            self.buffer.push(BufferedPacket {
                sequence: self.sequence,
                timestamp: self.timestamp,
                data: encoded_bytes.clone(),
            });
        }

        // Update state
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(self.config.samples_per_packet);

        encoded_bytes
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
    /// Returns error string if response invalid (legacy reasons, should probably be Result<(),
    /// Error>)
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
