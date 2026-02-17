use thiserror::Error;

/// RTP payload types for `AirPlay`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PayloadType {
    /// Timing request
    TimingRequest = 0x52,
    /// Timing response
    TimingResponse = 0x53,
    /// Audio data (realtime)
    AudioRealtime = 0x60,
    /// Audio data (buffered)
    AudioBuffered = 0x61,
    /// Retransmit request
    RetransmitRequest = 0x55,
    /// Retransmit response
    RetransmitResponse = 0x56,
}

impl PayloadType {
    /// Parse from byte value
    #[must_use]
    pub fn from_byte(b: u8) -> Option<Self> {
        match b & 0x7F {
            0x52 => Some(Self::TimingRequest),
            0x53 => Some(Self::TimingResponse),
            0x60 => Some(Self::AudioRealtime),
            0x61 => Some(Self::AudioBuffered),
            0x55 => Some(Self::RetransmitRequest),
            0x56 => Some(Self::RetransmitResponse),
            _ => None,
        }
    }
}

/// RTP header (12 bytes standard, extended for `AirPlay`)
#[derive(Debug, Clone)]
pub struct RtpHeader {
    /// Version (2 bits, always 2)
    pub version: u8,
    /// Padding flag
    pub padding: bool,
    /// Extension flag
    pub extension: bool,
    /// CSRC count (4 bits)
    pub csrc_count: u8,
    /// Marker bit
    pub marker: bool,
    /// Payload type (7 bits)
    pub payload_type: PayloadType,
    /// Sequence number (16 bits)
    pub sequence: u16,
    /// Timestamp (32 bits)
    pub timestamp: u32,
    /// Synchronization source ID (32 bits)
    pub ssrc: u32,
}

impl RtpHeader {
    /// Standard RTP header size
    pub const SIZE: usize = 12;

    /// Create a new audio packet header
    #[must_use]
    pub fn new_audio(sequence: u16, timestamp: u32, ssrc: u32, buffered: bool) -> Self {
        Self {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: true,
            payload_type: if buffered {
                PayloadType::AudioBuffered
            } else {
                PayloadType::AudioRealtime
            },
            sequence,
            timestamp,
            ssrc,
        }
    }

    /// Encode header to bytes
    #[must_use]
    pub fn encode(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];

        // Byte 0: V(2) | P(1) | X(1) | CC(4)
        buf[0] = (self.version << 6)
            | (u8::from(self.padding) << 5)
            | (u8::from(self.extension) << 4)
            | (self.csrc_count & 0x0F);

        // Byte 1: M(1) | PT(7)
        buf[1] = (u8::from(self.marker) << 7) | (self.payload_type as u8 & 0x7F);

        // Bytes 2-3: Sequence number
        buf[2..4].copy_from_slice(&self.sequence.to_be_bytes());

        // Bytes 4-7: Timestamp
        buf[4..8].copy_from_slice(&self.timestamp.to_be_bytes());

        // Bytes 8-11: SSRC
        buf[8..12].copy_from_slice(&self.ssrc.to_be_bytes());

        buf
    }

    /// Decode header from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small or version is invalid.
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < Self::SIZE {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: Self::SIZE,
                have: buf.len(),
            });
        }

        let version = (buf[0] >> 6) & 0x03;
        if version != 2 {
            return Err(RtpDecodeError::InvalidVersion(version));
        }

        let payload_type_byte = buf[1] & 0x7F;
        let payload_type = PayloadType::from_byte(payload_type_byte)
            .ok_or(RtpDecodeError::UnknownPayloadType(payload_type_byte))?;

        Ok(Self {
            version,
            padding: (buf[0] >> 5) & 0x01 != 0,
            extension: (buf[0] >> 4) & 0x01 != 0,
            csrc_count: buf[0] & 0x0F,
            marker: (buf[1] >> 7) & 0x01 != 0,
            payload_type,
            sequence: u16::from_be_bytes([buf[2], buf[3]]),
            timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            ssrc: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}

/// RTP decode errors
#[derive(Debug, Error)]
pub enum RtpDecodeError {
    #[error("buffer too small: need {needed} bytes, have {have}")]
    BufferTooSmall { needed: usize, have: usize },

    #[error("invalid RTP version: {0}")]
    InvalidVersion(u8),

    #[error("unknown payload type: 0x{0:02x}")]
    UnknownPayloadType(u8),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
}

/// Complete RTP packet with header and payload
#[derive(Debug, Clone)]
pub struct RtpPacket {
    /// Packet header
    pub header: RtpHeader,
    /// Payload data (audio samples or control data)
    pub payload: Vec<u8>,
}

impl RtpPacket {
    /// Create a new RTP packet
    #[must_use]
    pub fn new(header: RtpHeader, payload: Vec<u8>) -> Self {
        Self { header, payload }
    }

    /// Create an audio packet
    #[must_use]
    pub fn audio(
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        audio_data: Vec<u8>,
        buffered: bool,
    ) -> Self {
        Self {
            header: RtpHeader::new_audio(sequence, timestamp, ssrc, buffered),
            payload: audio_data,
        }
    }

    /// Encode packet to bytes (without encryption)
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(RtpHeader::SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.encode());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode packet from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small or header is invalid.
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        let header = RtpHeader::decode(buf)?;
        let payload = buf[RtpHeader::SIZE..].to_vec();
        Ok(Self { header, payload })
    }

    /// Get payload as audio samples (assuming 16-bit stereo)
    pub fn audio_samples(&self) -> impl Iterator<Item = (i16, i16)> + '_ {
        self.payload.chunks_exact(4).map(|chunk| {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            let right = i16::from_le_bytes([chunk[2], chunk[3]]);
            (left, right)
        })
    }
}
