//! RAOP-specific RTP packet types

use super::packet::RtpDecodeError;
use super::timing::NtpTimestamp;
use bytes::{BufMut, Bytes, BytesMut};

/// RAOP RTP payload types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RaopPayloadType {
    /// Timing request (client -> server)
    TimingRequest = 0x52,
    /// Timing response (server -> client)
    TimingResponse = 0x53,
    /// Sync packet (server -> client on control channel)
    Sync = 0x54,
    /// Retransmit request (server -> client on control channel)
    RetransmitRequest = 0x55,
    /// Retransmit response (client -> server, audio data)
    RetransmitResponse = 0x56,
    /// Audio data (realtime mode)
    AudioRealtime = 0x60,
    /// Audio data (buffered mode)
    AudioBuffered = 0x61,
}

impl RaopPayloadType {
    /// Parse from byte value
    #[must_use]
    pub fn from_byte(b: u8) -> Option<Self> {
        match b & 0x7F {
            0x52 => Some(Self::TimingRequest),
            0x53 => Some(Self::TimingResponse),
            0x54 => Some(Self::Sync),
            0x55 => Some(Self::RetransmitRequest),
            0x56 => Some(Self::RetransmitResponse),
            0x60 => Some(Self::AudioRealtime),
            0x61 => Some(Self::AudioBuffered),
            _ => None,
        }
    }

    /// Check if this is an audio payload type
    #[must_use]
    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::AudioRealtime | Self::AudioBuffered | Self::RetransmitResponse
        )
    }
}

/// RAOP sync packet (sent on control channel)
///
/// Provides synchronization between RTP timestamps and wall clock time.
#[derive(Debug, Clone)]
pub struct SyncPacket {
    /// Extension flag (set on first sync after RECORD/FLUSH)
    pub extension: bool,
    /// Current RTP timestamp being played
    pub rtp_timestamp: u32,
    /// Current NTP time
    pub ntp_time: NtpTimestamp,
    /// RTP timestamp of next audio packet
    pub next_timestamp: u32,
}

impl SyncPacket {
    /// Sync packet size (8-byte header + 4 + 8 + 4 = 24 bytes)
    pub const SIZE: usize = 20;

    /// Create a new sync packet
    #[must_use]
    pub fn new(
        rtp_timestamp: u32,
        ntp_time: NtpTimestamp,
        next_timestamp: u32,
        is_first: bool,
    ) -> Self {
        Self {
            extension: is_first,
            rtp_timestamp,
            ntp_time,
            next_timestamp,
        }
    }

    /// Encode to bytes
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);

        // RTP header (without SSRC)
        let flags = 0x80 | if self.extension { 0x10 } else { 0x00 };
        buf.push(flags);
        buf.push(0xD4); // Marker + PT=0x54

        // Sequence number (unused, set to 0x0007)
        buf.extend_from_slice(&0x0007u16.to_be_bytes());

        // RTP timestamp being played
        buf.extend_from_slice(&self.rtp_timestamp.to_be_bytes());

        // NTP timestamp (8 bytes)
        buf.extend_from_slice(&self.ntp_time.encode());

        // Next RTP timestamp
        buf.extend_from_slice(&self.next_timestamp.to_be_bytes());

        buf
    }

    /// Decode from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::SIZE,
                have: buf.len(),
            });
        }

        let extension = (buf[0] & 0x10) != 0;
        let rtp_timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ntp_time = NtpTimestamp::decode(&buf[8..16]);
        let next_timestamp = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);

        Ok(Self {
            extension,
            rtp_timestamp,
            ntp_time,
            next_timestamp,
        })
    }
}

/// Retransmit request packet
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub seq_start: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

impl RetransmitRequest {
    /// Packet size
    pub const SIZE: usize = 8;

    /// Decode from bytes (after 8-byte header)
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        Ok(Self {
            seq_start: u16::from_be_bytes([buf[0], buf[1]]),
            count: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }
}

/// RAOP audio packet with header
#[derive(Debug, Clone)]
pub struct RaopAudioPacket {
    /// Marker bit (set on first packet after RECORD/FLUSH)
    pub marker: bool,
    /// Sequence number
    pub sequence: u16,
    /// RTP timestamp
    pub timestamp: u32,
    /// SSRC
    pub ssrc: u32,
    /// Audio payload (encrypted)
    pub payload: Bytes,
}

impl RaopAudioPacket {
    /// RTP header size
    pub const HEADER_SIZE: usize = 12;

    /// Create a new audio packet
    #[must_use]
    pub fn new(sequence: u16, timestamp: u32, ssrc: u32, payload: impl Into<Bytes>) -> Self {
        Self {
            marker: false,
            sequence,
            timestamp,
            ssrc,
            payload: payload.into(),
        }
    }

    /// Set marker bit (first packet after RECORD/FLUSH)
    #[must_use]
    pub fn with_marker(mut self) -> Self {
        self.marker = true;
        self
    }

    /// Encode to bytes
    #[must_use]
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(Self::HEADER_SIZE + self.payload.len());

        // RTP header
        buf.put_u8(0x80); // V=2, P=0, X=0, CC=0
        buf.put_u8(0x60 | if self.marker { 0x80 } else { 0x00 }); // PT=0x60, M bit

        buf.put_u16(self.sequence);
        buf.put_u32(self.timestamp);
        buf.put_u32(self.ssrc);

        // Payload
        buf.put_slice(&self.payload);

        buf.freeze()
    }

    /// Decode from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::HEADER_SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::HEADER_SIZE,
                have: buf.len(),
            });
        }

        let marker = (buf[1] & 0x80) != 0;
        let sequence = u16::from_be_bytes([buf[2], buf[3]]);
        let timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ssrc = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let payload = Bytes::copy_from_slice(&buf[Self::HEADER_SIZE..]);

        Ok(Self {
            marker,
            sequence,
            timestamp,
            ssrc,
            payload,
        })
    }
}
