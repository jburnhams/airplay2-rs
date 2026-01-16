use super::packet::RtpDecodeError;

/// Retransmit request for lost packets
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub sequence_start: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

impl RetransmitRequest {
    /// Create a new retransmit request
    pub fn new(sequence_start: u16, count: u16) -> Self {
        Self {
            sequence_start,
            count,
        }
    }

    /// Encode to bytes (including RTP-like header)
    pub fn encode(&self, ssrc: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16);

        // Header
        buf.push(0x80);
        buf.push(0xD5); // PT=0x55 (retransmit request)
        buf.extend_from_slice(&self.sequence_start.to_be_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Timestamp
        buf.extend_from_slice(&ssrc.to_be_bytes());

        // Retransmit data
        buf.extend_from_slice(&self.sequence_start.to_be_bytes());
        buf.extend_from_slice(&self.count.to_be_bytes());

        buf
    }

    /// Decode from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        Ok(Self {
            sequence_start: u16::from_be_bytes([buf[0], buf[1]]),
            count: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }
}

/// Control packet types
#[derive(Debug, Clone)]
pub enum ControlPacket {
    /// Request retransmission of lost packets
    RetransmitRequest(RetransmitRequest),
    /// Sync packet for timing
    Sync {
        rtp_timestamp: u32,
        ntp_timestamp: super::timing::NtpTimestamp,
        next_timestamp: u32,
    },
}

impl ControlPacket {
    /// Parse control packet from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 12 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 12,
                have: buf.len(),
            });
        }

        let payload_type = buf[1] & 0x7F;

        match payload_type {
            0x55 => {
                let request = RetransmitRequest::decode(&buf[12..])?;
                Ok(ControlPacket::RetransmitRequest(request))
            }
            0x54 => {
                // Sync packet
                if buf.len() < 20 {
                    return Err(RtpDecodeError::BufferTooSmall {
                        needed: 20,
                        have: buf.len(),
                    });
                }
                Ok(ControlPacket::Sync {
                    rtp_timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
                    ntp_timestamp: super::timing::NtpTimestamp::decode(&buf[8..16]),
                    next_timestamp: u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]),
                })
            }
            _ => Err(RtpDecodeError::UnknownPayloadType(payload_type)),
        }
    }
}
