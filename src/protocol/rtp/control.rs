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
    #[must_use]
    pub fn new(sequence_start: u16, count: u16) -> Self {
        Self {
            sequence_start,
            count,
        }
    }

    /// Encode to bytes (including RTP-like header)
    #[must_use]
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
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small.
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
    /// `AirPlay` 2 PTP Time Announce packet
    TimeAnnouncePtp {
        rtp_timestamp: u32,
        ptp_timestamp: u64,
        rtp_timestamp_next: u32,
        clock_identity: u64,
    },
}

impl ControlPacket {
    /// Encode packet to bytes
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ControlPacket::RetransmitRequest(req) => req.encode(0), // SSRC 0 placeholder?
            ControlPacket::Sync { .. } => Vec::new(),               /* Not implemented for
                                                                      * encoding yet */
            ControlPacket::TimeAnnouncePtp {
                rtp_timestamp,
                ptp_timestamp,
                rtp_timestamp_next,
                clock_identity,
            } => {
                let mut buf = Vec::with_capacity(28);
                // Header (V=2, P=0, RC=0, PT=215 (0xD7), Length=6)
                buf.push(0x80);
                buf.push(0xD7);
                buf.extend_from_slice(&6u16.to_be_bytes());

                // Payload
                buf.extend_from_slice(&rtp_timestamp.to_be_bytes());
                buf.extend_from_slice(&ptp_timestamp.to_be_bytes());
                buf.extend_from_slice(&rtp_timestamp_next.to_be_bytes());
                buf.extend_from_slice(&clock_identity.to_be_bytes());

                buf
            }
        }
    }

    /// Parse control packet from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small or payload type is unknown.
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        // Check for full byte payload type for extended types (AirPlay 2)
        // Or masked for legacy
        let payload_type_masked = buf[1] & 0x7F;
        let payload_type_full = buf[1];

        if payload_type_full == 0xD7 {
            // TimeAnnouncePtp (215)
            if buf.len() < 28 {
                return Err(RtpDecodeError::BufferTooSmall {
                    needed: 28,
                    have: buf.len(),
                });
            }
            return Ok(ControlPacket::TimeAnnouncePtp {
                rtp_timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
                ptp_timestamp: u64::from_be_bytes([
                    buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
                ]),
                rtp_timestamp_next: u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]),
                clock_identity: u64::from_be_bytes([
                    buf[20], buf[21], buf[22], buf[23], buf[24], buf[25], buf[26], buf[27],
                ]),
            });
        }

        match payload_type_masked {
            0x55 => {
                if buf.len() < 12 {
                    return Err(RtpDecodeError::BufferTooSmall {
                        needed: 12,
                        have: buf.len(),
                    });
                }
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
            _ => Err(RtpDecodeError::UnknownPayloadType(payload_type_masked)),
        }
    }
}
