//! PTP message types, parsing, and encoding.
//!
//! Implements IEEE 1588 PTP message format with AirPlay extensions.
//! Supports both the standard 34-byte header format and the compact
//! AirPlay timing packet format.

use super::timestamp::PtpTimestamp;

/// PTP message type identifiers (IEEE 1588 Section 13.3.2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PtpMessageType {
    /// Sync message (master → slave), carries T1.
    Sync = 0x00,
    /// Delay request (slave → master), sent at T3.
    DelayReq = 0x01,
    /// Follow-up (master → slave), carries precise T1.
    FollowUp = 0x08,
    /// Delay response (master → slave), carries T4.
    DelayResp = 0x09,
    /// Announce (master → slave), clock properties.
    Announce = 0x0B,
}

impl PtpMessageType {
    /// Parse from the lower 4 bits of a byte.
    pub fn from_nibble(value: u8) -> Result<Self, PtpParseError> {
        match value & 0x0F {
            0x00 => Ok(Self::Sync),
            0x01 => Ok(Self::DelayReq),
            0x08 => Ok(Self::FollowUp),
            0x09 => Ok(Self::DelayResp),
            0x0B => Ok(Self::Announce),
            other => Err(PtpParseError::UnknownMessageType(other)),
        }
    }

    /// Whether this message type is an event message (requires timestamping).
    #[must_use]
    pub fn is_event(&self) -> bool {
        matches!(self, Self::Sync | Self::DelayReq)
    }

    /// Whether this message type is a general message.
    #[must_use]
    pub fn is_general(&self) -> bool {
        !self.is_event()
    }
}

impl std::fmt::Display for PtpMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "Sync"),
            Self::DelayReq => write!(f, "Delay_Req"),
            Self::FollowUp => write!(f, "Follow_Up"),
            Self::DelayResp => write!(f, "Delay_Resp"),
            Self::Announce => write!(f, "Announce"),
        }
    }
}

/// PTP port identity: 8-byte clock ID + 2-byte port number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PtpPortIdentity {
    /// 8-byte clock identity (typically derived from MAC address).
    pub clock_identity: u64,
    /// Port number (1-based).
    pub port_number: u16,
}

impl PtpPortIdentity {
    /// Create a new port identity.
    #[must_use]
    pub fn new(clock_identity: u64, port_number: u16) -> Self {
        Self {
            clock_identity,
            port_number,
        }
    }

    /// Encode as 10 bytes (8-byte clock ID + 2-byte port number, BE).
    #[must_use]
    pub fn encode(&self) -> [u8; 10] {
        let mut buf = [0u8; 10];
        buf[0..8].copy_from_slice(&self.clock_identity.to_be_bytes());
        buf[8..10].copy_from_slice(&self.port_number.to_be_bytes());
        buf
    }

    /// Decode from 10 bytes.
    #[must_use]
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 10 {
            return None;
        }
        Some(Self {
            clock_identity: u64::from_be_bytes([
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            ]),
            port_number: u16::from_be_bytes([data[8], data[9]]),
        })
    }
}

/// Full IEEE 1588 PTP message header (34 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtpHeader {
    /// Transport-specific nibble (upper 4 bits of byte 0).
    pub transport_specific: u8,
    /// Message type (lower 4 bits of byte 0).
    pub message_type: PtpMessageType,
    /// PTP version (typically 2).
    pub version: u8,
    /// Total message length including header.
    pub message_length: u16,
    /// Domain number.
    pub domain_number: u8,
    /// Flags field.
    pub flags: u16,
    /// Correction field (nanoseconds * 2^16, signed).
    pub correction_field: i64,
    /// Source port identity.
    pub source_port_identity: PtpPortIdentity,
    /// Sequence ID.
    pub sequence_id: u16,
    /// Control field (deprecated in v2, but still present).
    pub control_field: u8,
    /// Log message interval.
    pub log_message_interval: i8,
}

impl PtpHeader {
    /// Header size in bytes.
    pub const SIZE: usize = 34;

    /// Default PTP version.
    pub const PTP_VERSION_2: u8 = 2;

    /// Create a new header with sensible defaults.
    #[must_use]
    pub fn new(message_type: PtpMessageType, source: PtpPortIdentity, sequence_id: u16) -> Self {
        let control_field = match message_type {
            PtpMessageType::Sync => 0x00,
            PtpMessageType::DelayReq => 0x01,
            PtpMessageType::FollowUp => 0x02,
            PtpMessageType::DelayResp => 0x03,
            PtpMessageType::Announce => 0x05,
        };
        Self {
            transport_specific: 0,
            message_type,
            version: Self::PTP_VERSION_2,
            message_length: 0, // filled in on encode
            domain_number: 0,
            flags: 0,
            correction_field: 0,
            source_port_identity: source,
            sequence_id,
            control_field,
            log_message_interval: 0,
        }
    }

    /// Encode to 34 bytes.
    #[must_use]
    pub fn encode(&self, body_length: usize) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = (self.transport_specific << 4) | (self.message_type as u8 & 0x0F);
        buf[1] = self.version & 0x0F;
        let total_len = (Self::SIZE + body_length) as u16;
        buf[2..4].copy_from_slice(&total_len.to_be_bytes());
        buf[4] = self.domain_number;
        // buf[5] reserved
        buf[6..8].copy_from_slice(&self.flags.to_be_bytes());
        buf[8..16].copy_from_slice(&self.correction_field.to_be_bytes());
        // buf[16..20] reserved
        let port_id = self.source_port_identity.encode();
        buf[20..30].copy_from_slice(&port_id);
        buf[30..32].copy_from_slice(&self.sequence_id.to_be_bytes());
        buf[32] = self.control_field;
        buf[33] = self.log_message_interval as u8;
        buf
    }

    /// Decode from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, PtpParseError> {
        if data.len() < Self::SIZE {
            return Err(PtpParseError::TooShort {
                needed: Self::SIZE,
                have: data.len(),
            });
        }
        let message_type = PtpMessageType::from_nibble(data[0])?;
        let source_port_identity =
            PtpPortIdentity::decode(&data[20..30]).ok_or(PtpParseError::TooShort {
                needed: 30,
                have: data.len(),
            })?;
        Ok(Self {
            transport_specific: data[0] >> 4,
            message_type,
            version: data[1] & 0x0F,
            message_length: u16::from_be_bytes([data[2], data[3]]),
            domain_number: data[4],
            flags: u16::from_be_bytes([data[6], data[7]]),
            correction_field: i64::from_be_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
            source_port_identity,
            sequence_id: u16::from_be_bytes([data[30], data[31]]),
            control_field: data[32],
            log_message_interval: data[33] as i8,
        })
    }
}

/// A parsed PTP message with header and typed body.
#[derive(Debug, Clone)]
pub struct PtpMessage {
    /// Message header.
    pub header: PtpHeader,
    /// Message body.
    pub body: PtpMessageBody,
}

/// PTP message body variants.
#[derive(Debug, Clone)]
pub enum PtpMessageBody {
    /// Sync: origin timestamp (T1 if one-step, or approximate if two-step).
    Sync {
        /// Origin timestamp.
        origin_timestamp: PtpTimestamp,
    },
    /// Follow-up: precise origin timestamp (T1).
    FollowUp {
        /// Precise origin timestamp from the associated Sync.
        precise_origin_timestamp: PtpTimestamp,
    },
    /// Delay request: origin timestamp (T3).
    DelayReq {
        /// Origin timestamp.
        origin_timestamp: PtpTimestamp,
    },
    /// Delay response: receive timestamp (T4) and requesting port identity.
    DelayResp {
        /// Receive timestamp (when master received the Delay_Req).
        receive_timestamp: PtpTimestamp,
        /// Port identity of the requester.
        requesting_port_identity: PtpPortIdentity,
    },
    /// Announce: clock properties.
    Announce {
        /// Origin timestamp.
        origin_timestamp: PtpTimestamp,
        /// Grandmaster clock identity.
        grandmaster_identity: u64,
        /// Grandmaster clock quality.
        grandmaster_priority1: u8,
        /// Grandmaster clock quality.
        grandmaster_priority2: u8,
    },
}

impl PtpMessage {
    /// Body size for Sync/FollowUp/DelayReq (10-byte timestamp).
    const TIMESTAMP_BODY_SIZE: usize = 10;
    /// Body size for DelayResp (10-byte timestamp + 10-byte port identity).
    const DELAY_RESP_BODY_SIZE: usize = 20;
    /// Body size for Announce (10-byte timestamp + 10 bytes of clock properties).
    const ANNOUNCE_BODY_SIZE: usize = 20;

    /// Parse a complete PTP message from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, PtpParseError> {
        let header = PtpHeader::decode(data)?;
        let body_data = &data[PtpHeader::SIZE..];

        let body = match header.message_type {
            PtpMessageType::Sync => {
                let ts =
                    PtpTimestamp::decode_ieee1588(body_data).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::TIMESTAMP_BODY_SIZE,
                        have: data.len(),
                    })?;
                PtpMessageBody::Sync {
                    origin_timestamp: ts,
                }
            }
            PtpMessageType::FollowUp => {
                let ts =
                    PtpTimestamp::decode_ieee1588(body_data).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::TIMESTAMP_BODY_SIZE,
                        have: data.len(),
                    })?;
                PtpMessageBody::FollowUp {
                    precise_origin_timestamp: ts,
                }
            }
            PtpMessageType::DelayReq => {
                let ts =
                    PtpTimestamp::decode_ieee1588(body_data).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::TIMESTAMP_BODY_SIZE,
                        have: data.len(),
                    })?;
                PtpMessageBody::DelayReq {
                    origin_timestamp: ts,
                }
            }
            PtpMessageType::DelayResp => {
                if body_data.len() < Self::DELAY_RESP_BODY_SIZE {
                    return Err(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::DELAY_RESP_BODY_SIZE,
                        have: data.len(),
                    });
                }
                let ts =
                    PtpTimestamp::decode_ieee1588(body_data).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::DELAY_RESP_BODY_SIZE,
                        have: data.len(),
                    })?;
                let port_id =
                    PtpPortIdentity::decode(&body_data[10..20]).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::DELAY_RESP_BODY_SIZE,
                        have: data.len(),
                    })?;
                PtpMessageBody::DelayResp {
                    receive_timestamp: ts,
                    requesting_port_identity: port_id,
                }
            }
            PtpMessageType::Announce => {
                if body_data.len() < Self::ANNOUNCE_BODY_SIZE {
                    return Err(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::ANNOUNCE_BODY_SIZE,
                        have: data.len(),
                    });
                }
                let ts =
                    PtpTimestamp::decode_ieee1588(body_data).ok_or(PtpParseError::TooShort {
                        needed: PtpHeader::SIZE + Self::ANNOUNCE_BODY_SIZE,
                        have: data.len(),
                    })?;
                let gm_identity = u64::from_be_bytes([
                    body_data[10],
                    body_data[11],
                    body_data[12],
                    body_data[13],
                    body_data[14],
                    body_data[15],
                    body_data[16],
                    body_data[17],
                ]);
                PtpMessageBody::Announce {
                    origin_timestamp: ts,
                    grandmaster_identity: gm_identity,
                    grandmaster_priority1: body_data[18],
                    grandmaster_priority2: body_data[19],
                }
            }
        };

        Ok(Self { header, body })
    }

    /// Encode to bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let body_bytes = self.encode_body();
        let header_bytes = self.header.encode(body_bytes.len());
        let mut buf = Vec::with_capacity(PtpHeader::SIZE + body_bytes.len());
        buf.extend_from_slice(&header_bytes);
        buf.extend_from_slice(&body_bytes);
        buf
    }

    fn encode_body(&self) -> Vec<u8> {
        match &self.body {
            PtpMessageBody::Sync { origin_timestamp }
            | PtpMessageBody::FollowUp {
                precise_origin_timestamp: origin_timestamp,
            }
            | PtpMessageBody::DelayReq { origin_timestamp } => {
                origin_timestamp.encode_ieee1588().to_vec()
            }
            PtpMessageBody::DelayResp {
                receive_timestamp,
                requesting_port_identity,
            } => {
                let mut buf = Vec::with_capacity(Self::DELAY_RESP_BODY_SIZE);
                buf.extend_from_slice(&receive_timestamp.encode_ieee1588());
                buf.extend_from_slice(&requesting_port_identity.encode());
                buf
            }
            PtpMessageBody::Announce {
                origin_timestamp,
                grandmaster_identity,
                grandmaster_priority1,
                grandmaster_priority2,
            } => {
                let mut buf = Vec::with_capacity(Self::ANNOUNCE_BODY_SIZE);
                buf.extend_from_slice(&origin_timestamp.encode_ieee1588());
                buf.extend_from_slice(&grandmaster_identity.to_be_bytes());
                buf.push(*grandmaster_priority1);
                buf.push(*grandmaster_priority2);
                buf
            }
        }
    }

    /// Create a Sync message.
    #[must_use]
    pub fn sync(source: PtpPortIdentity, sequence_id: u16, timestamp: PtpTimestamp) -> Self {
        Self {
            header: PtpHeader::new(PtpMessageType::Sync, source, sequence_id),
            body: PtpMessageBody::Sync {
                origin_timestamp: timestamp,
            },
        }
    }

    /// Create a Follow-up message.
    #[must_use]
    pub fn follow_up(
        source: PtpPortIdentity,
        sequence_id: u16,
        precise_timestamp: PtpTimestamp,
    ) -> Self {
        Self {
            header: PtpHeader::new(PtpMessageType::FollowUp, source, sequence_id),
            body: PtpMessageBody::FollowUp {
                precise_origin_timestamp: precise_timestamp,
            },
        }
    }

    /// Create a Delay Request message.
    #[must_use]
    pub fn delay_req(source: PtpPortIdentity, sequence_id: u16, timestamp: PtpTimestamp) -> Self {
        Self {
            header: PtpHeader::new(PtpMessageType::DelayReq, source, sequence_id),
            body: PtpMessageBody::DelayReq {
                origin_timestamp: timestamp,
            },
        }
    }

    /// Create a Delay Response message.
    #[must_use]
    pub fn delay_resp(
        source: PtpPortIdentity,
        sequence_id: u16,
        receive_timestamp: PtpTimestamp,
        requesting_port: PtpPortIdentity,
    ) -> Self {
        Self {
            header: PtpHeader::new(PtpMessageType::DelayResp, source, sequence_id),
            body: PtpMessageBody::DelayResp {
                receive_timestamp,
                requesting_port_identity: requesting_port,
            },
        }
    }

    /// Create an Announce message.
    #[must_use]
    pub fn announce(
        source: PtpPortIdentity,
        sequence_id: u16,
        grandmaster_identity: u64,
        priority1: u8,
        priority2: u8,
    ) -> Self {
        Self {
            header: PtpHeader::new(PtpMessageType::Announce, source, sequence_id),
            body: PtpMessageBody::Announce {
                origin_timestamp: PtpTimestamp::now(),
                grandmaster_identity,
                grandmaster_priority1: priority1,
                grandmaster_priority2: priority2,
            },
        }
    }
}

// --- Compact AirPlay timing packet ---

/// Compact AirPlay PTP timing packet (24 bytes on wire).
///
/// This is the simplified format used by AirPlay 2 for timing exchanges
/// on the timing channel port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirPlayTimingPacket {
    /// Message type.
    pub message_type: PtpMessageType,
    /// Sequence ID.
    pub sequence_id: u16,
    /// Timestamp in AirPlay compact format (48.16 fixed-point).
    pub timestamp: PtpTimestamp,
    /// Clock identity.
    pub clock_id: u64,
}

impl AirPlayTimingPacket {
    /// Wire size in bytes.
    pub const SIZE: usize = 24;

    /// Parse from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, PtpParseError> {
        if data.len() < 22 {
            return Err(PtpParseError::TooShort {
                needed: 22,
                have: data.len(),
            });
        }
        let message_type = PtpMessageType::from_nibble(data[0])?;
        let sequence_id = u16::from_be_bytes([data[2], data[3]]);
        let compact_ts = u64::from_be_bytes([
            0, 0, data[8], data[9], data[10], data[11], data[12], data[13],
        ]);
        let timestamp = PtpTimestamp::from_airplay_compact(compact_ts);

        let clock_id = if data.len() >= 22 {
            u64::from_be_bytes([
                data[14], data[15], data[16], data[17], data[18], data[19], data[20], data[21],
            ])
        } else {
            0
        };

        Ok(Self {
            message_type,
            sequence_id,
            timestamp,
            clock_id,
        })
    }

    /// Encode to bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut data = [0u8; Self::SIZE];
        data[0] = self.message_type as u8;
        data[2..4].copy_from_slice(&self.sequence_id.to_be_bytes());
        let compact = self.timestamp.to_airplay_compact();
        let ts_bytes = compact.to_be_bytes();
        data[8..14].copy_from_slice(&ts_bytes[2..8]);
        data[14..22].copy_from_slice(&self.clock_id.to_be_bytes());
        data
    }
}

/// Errors from PTP message parsing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PtpParseError {
    /// Packet too short.
    #[error("packet too short: need {needed} bytes, have {have}")]
    TooShort {
        /// Minimum bytes needed.
        needed: usize,
        /// Bytes actually available.
        have: usize,
    },
    /// Unknown message type.
    #[error("unknown PTP message type: 0x{0:02X}")]
    UnknownMessageType(u8),
}
