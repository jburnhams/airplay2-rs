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

/// Time announcement packet using PTP reference (Type 215)
///
/// Used to synchronize RTP timestamp with PTP monotonic time.
#[derive(Debug, Clone)]
pub struct TimeAnnouncePtp {
    /// RTP timestamp at the PTP time
    pub rtp_timestamp: u32,
    /// PTP monotonic timestamp (nanoseconds)
    pub ptp_timestamp: u64,
    /// Next play-at RTP timestamp
    pub next_rtp_timestamp: u32,
    /// PTP clock identity (Grandmaster ID)
    pub clock_identity: u64,
}

impl TimeAnnouncePtp {
    /// Create a new TimeAnnouncePtp packet
    pub fn new(
        rtp_timestamp: u32,
        ptp_timestamp: u64,
        next_rtp_timestamp: u32,
        clock_identity: u64,
    ) -> Self {
        Self {
            rtp_timestamp,
            ptp_timestamp,
            next_rtp_timestamp,
            clock_identity,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        // Total size 28 bytes
        // Header: 4 bytes
        // Body: 24 bytes
        let mut buf = Vec::with_capacity(28);

        // Header
        buf.push(0x80); // Version 2
        buf.push(0xD7); // Type 215 (TimeAnnouncePtp)
        // Length in 32-bit words minus 1. 28 bytes = 7 words. Length = 6.
        buf.extend_from_slice(&0x0006u16.to_be_bytes());

        // Body
        buf.extend_from_slice(&self.rtp_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ptp_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.next_rtp_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.clock_identity.to_be_bytes());

        buf
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
    /// PTP Time Announcement
    TimeAnnouncePtp(TimeAnnouncePtp),
}

impl ControlPacket {
    /// Parse control packet from bytes
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 4 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 4,
                have: buf.len(),
            });
        }

        let payload_type = buf[1]; // Use raw byte for extended types

        match payload_type {
            0x55 | 0xD5 => {
                // Retransmit Request
                if buf.len() < 12 {
                    return Err(RtpDecodeError::BufferTooSmall {
                        needed: 12,
                        have: buf.len(),
                    });
                }
                // Skip header (12 bytes)
                let request = RetransmitRequest::decode(&buf[12..])?;
                Ok(ControlPacket::RetransmitRequest(request))
            }
            0x54 | 0xD4 => {
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
            0xD7 => {
                // TimeAnnouncePtp
                if buf.len() < 28 {
                    return Err(RtpDecodeError::BufferTooSmall {
                        needed: 28,
                        have: buf.len(),
                    });
                }
                Ok(ControlPacket::TimeAnnouncePtp(TimeAnnouncePtp {
                    rtp_timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
                    ptp_timestamp: u64::from_be_bytes([
                        buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
                    ]),
                    next_rtp_timestamp: u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]),
                    clock_identity: u64::from_be_bytes([
                        buf[20], buf[21], buf[22], buf[23], buf[24], buf[25], buf[26], buf[27],
                    ]),
                }))
            }
            _ => Err(RtpDecodeError::UnknownPayloadType(payload_type & 0x7F)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_announce_ptp_encode() {
        let packet = TimeAnnouncePtp {
            rtp_timestamp: 0x11223344,
            ptp_timestamp: 0x5566778899AABBCC,
            next_rtp_timestamp: 0xDDEEFF00,
            clock_identity: 0x123456789ABCDEF0,
        };

        let encoded = packet.encode();

        assert_eq!(encoded.len(), 28);
        assert_eq!(encoded[0], 0x80);
        assert_eq!(encoded[1], 0xD7);
        assert_eq!(encoded[2], 0x00);
        assert_eq!(encoded[3], 0x06);

        // RTP Timestamp
        assert_eq!(&encoded[4..8], &[0x11, 0x22, 0x33, 0x44]);
        // PTP Timestamp
        assert_eq!(
            &encoded[8..16],
            &[0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC]
        );
        // Next RTP Timestamp
        assert_eq!(&encoded[16..20], &[0xDD, 0xEE, 0xFF, 0x00]);
        // Clock Identity
        assert_eq!(
            &encoded[20..28],
            &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]
        );
    }

    #[test]
    fn test_time_announce_ptp_decode() {
        let data = vec![
            0x80, 0xD7, 0x00, 0x06, // Header
            0x11, 0x22, 0x33, 0x44, // RTP Timestamp
            0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, // PTP Timestamp
            0xDD, 0xEE, 0xFF, 0x00, // Next RTP Timestamp
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, // Clock Identity
        ];

        let decoded = ControlPacket::decode(&data).unwrap();

        if let ControlPacket::TimeAnnouncePtp(packet) = decoded {
            assert_eq!(packet.rtp_timestamp, 0x11223344);
            assert_eq!(packet.ptp_timestamp, 0x5566778899AABBCC);
            assert_eq!(packet.next_rtp_timestamp, 0xDDEEFF00);
            assert_eq!(packet.clock_identity, 0x123456789ABCDEF0);
        } else {
            panic!("Expected TimeAnnouncePtp packet");
        }
    }
}
