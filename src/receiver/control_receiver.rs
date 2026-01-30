//! Control port receiver
//!
//! Handles sync packets and retransmission requests on the control UDP port.

use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

/// Control packet types
const PACKET_TYPE_SYNC: u8 = 0x54;
const PACKET_TYPE_RETRANSMIT_REQUEST: u8 = 0x55;

/// Sync packet from sender
#[derive(Debug, Clone)]
pub struct SyncPacket {
    /// Extension bit
    pub extension: bool,
    /// RTP timestamp at next packet
    pub rtp_timestamp: u32,
    /// NTP timestamp (when sent)
    pub ntp_timestamp: u64,
    /// RTP timestamp at NTP time
    pub rtp_timestamp_at_ntp: u32,
}

/// Retransmit request (we receive these; respond on control port)
#[derive(Debug, Clone)]
pub struct RetransmitRequest {
    /// First sequence number to retransmit
    pub first_seq: u16,
    /// Number of packets to retransmit
    pub count: u16,
}

/// Events from control port
#[derive(Debug, Clone)]
pub enum ControlEvent {
    /// Sync packet
    Sync(SyncPacket),
    /// Retransmit request
    RetransmitRequest(RetransmitRequest),
}

/// Control port receiver
pub struct ControlReceiver {
    socket: Arc<UdpSocket>,
    event_tx: mpsc::Sender<ControlEvent>,
}

impl ControlReceiver {
    /// Create a new control receiver
    #[must_use]
    pub fn new(socket: Arc<UdpSocket>, event_tx: mpsc::Sender<ControlEvent>) -> Self {
        Self { socket, event_tx }
    }

    /// Run the receive loop
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket access fails.
    pub async fn run(self) -> Result<(), std::io::Error> {
        let mut buf = [0u8; 256];

        loop {
            let (len, _src) = self.socket.recv_from(&mut buf).await?;

            if len < 8 {
                continue;
            }

            if let Some(event) = Self::parse_packet(&buf[..len]) {
                if self.event_tx.send(event).await.is_err() {
                    break;
                }
            }
        }

        Ok(())
    }

    fn parse_packet(data: &[u8]) -> Option<ControlEvent> {
        if data.len() < 8 {
            return None;
        }

        let packet_type = data[1] & 0x7F;

        match packet_type {
            PACKET_TYPE_SYNC => Self::parse_sync(data),
            PACKET_TYPE_RETRANSMIT_REQUEST => Self::parse_retransmit(data),
            _ => None,
        }
    }

    fn parse_sync(data: &[u8]) -> Option<ControlEvent> {
        // Sync packet format:
        // Byte 0: 0x80 | extension bit
        // Byte 1: 0x54 (marker + type)
        // Bytes 2-3: sequence (ignored)
        // Bytes 4-7: RTP timestamp (next packet)
        // Bytes 8-15: NTP timestamp
        // Bytes 16-19: RTP timestamp at NTP time

        if data.len() < 20 {
            return None;
        }

        let extension = (data[0] & 0x10) != 0;
        let rtp_timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ntp_timestamp = u64::from_be_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);
        let rtp_timestamp_at_ntp = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);

        Some(ControlEvent::Sync(SyncPacket {
            extension,
            rtp_timestamp,
            ntp_timestamp,
            rtp_timestamp_at_ntp,
        }))
    }

    fn parse_retransmit(data: &[u8]) -> Option<ControlEvent> {
        // Retransmit request format:
        // Bytes 0-1: Header
        // Bytes 4-5: Sequence of missing packet
        // Bytes 6-7: Count

        if data.len() < 8 {
            return None;
        }

        let first_seq = u16::from_be_bytes([data[4], data[5]]);
        let count = u16::from_be_bytes([data[6], data[7]]);

        Some(ControlEvent::RetransmitRequest(RetransmitRequest {
            first_seq,
            count,
        }))
    }
}
