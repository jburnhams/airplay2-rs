//! NTP client implementation (RFC 5905)

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::error::AirPlayError;
use crate::protocol::rtp::timing::NtpTimestamp;

/// NTP packet size
pub const NTP_PACKET_SIZE: usize = 48;

/// NTP packet structure
#[derive(Debug, Clone, Default)]
pub struct NtpPacket {
    /// Leap Indicator, Version Number, and Mode
    pub li_vn_mode: u8,
    /// Stratum level
    pub stratum: u8,
    /// Poll interval
    pub poll: u8,
    /// Precision
    pub precision: i8,
    /// Root delay
    pub root_delay: u32,
    /// Root dispersion
    pub root_dispersion: u32,
    /// Reference identifier
    pub reference_id: u32,
    /// Reference timestamp
    pub reference_timestamp: NtpTimestamp,
    /// Origin timestamp
    pub origin_timestamp: NtpTimestamp,
    /// Receive timestamp
    pub receive_timestamp: NtpTimestamp,
    /// Transmit timestamp
    pub transmit_timestamp: NtpTimestamp,
}

impl NtpPacket {
    /// Create a new NTP client request packet
    #[must_use]
    pub fn new_client_request() -> Self {
        let mut packet = Self::default();
        // LI = 0 (no warning), VN = 4 (IPv4/IPv6), Mode = 3 (Client)
        // 00 100 011 = 0x23
        packet.li_vn_mode = 0x23;
        packet.transmit_timestamp = NtpTimestamp::now();
        packet
    }

    /// Encode the packet into 48 bytes
    #[must_use]
    pub fn encode(&self) -> [u8; NTP_PACKET_SIZE] {
        let mut buf = [0u8; NTP_PACKET_SIZE];
        buf[0] = self.li_vn_mode;
        buf[1] = self.stratum;
        buf[2] = self.poll;
        buf[3] = self.precision as u8;
        buf[4..8].copy_from_slice(&self.root_delay.to_be_bytes());
        buf[8..12].copy_from_slice(&self.root_dispersion.to_be_bytes());
        buf[12..16].copy_from_slice(&self.reference_id.to_be_bytes());
        buf[16..24].copy_from_slice(&self.reference_timestamp.encode());
        buf[24..32].copy_from_slice(&self.origin_timestamp.encode());
        buf[32..40].copy_from_slice(&self.receive_timestamp.encode());
        buf[40..48].copy_from_slice(&self.transmit_timestamp.encode());
        buf
    }

    /// Decode a packet from bytes
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer is less than 48 bytes
    pub fn decode(buf: &[u8]) -> Result<Self, std::io::Error> {
        if buf.len() < NTP_PACKET_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "NTP packet too small",
            ));
        }

        Ok(Self {
            li_vn_mode: buf[0],
            stratum: buf[1],
            poll: buf[2],
            precision: buf[3] as i8,
            root_delay: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            root_dispersion: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
            reference_id: u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]),
            reference_timestamp: NtpTimestamp::decode(&buf[16..24]),
            origin_timestamp: NtpTimestamp::decode(&buf[24..32]),
            receive_timestamp: NtpTimestamp::decode(&buf[32..40]),
            transmit_timestamp: NtpTimestamp::decode(&buf[40..48]),
        })
    }
}

/// NTP Client for querying a server's time and computing the clock offset
#[derive(Debug)]
pub struct NtpClient {
    /// Socket for sending/receiving NTP packets
    socket: UdpSocket,
}

impl NtpClient {
    /// Create a new NTP client bound to an ephemeral UDP port
    pub async fn new() -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(Self { socket })
    }

    /// Retrieve the clock offset from the specified server.
    /// Returns the (offset_micros, rtt_micros) where `offset_micros` is
    /// server_time - local_time.
    pub async fn get_offset(
        &self,
        server_addr: SocketAddr,
        timeout_duration: Duration,
    ) -> Result<(i64, u64), AirPlayError> {
        let request = NtpPacket::new_client_request();
        let encoded = request.encode();

        let t1 = request.transmit_timestamp;

        self.socket
            .send_to(&encoded, server_addr)
            .await
            .map_err(|e| AirPlayError::RtspError {
                message: format!("Failed to send NTP request: {e}"),
                status_code: None,
            })?;

        let mut buf = [0u8; 1024];
        let recv_future = self.socket.recv_from(&mut buf);
        let (len, _src) = timeout(timeout_duration, recv_future)
            .await
            .map_err(|_| AirPlayError::RtspError {
                message: "NTP request timed out".to_string(),
                status_code: None,
            })?
            .map_err(|e| AirPlayError::RtspError {
                message: format!("Failed to receive NTP response: {e}"),
                status_code: None,
            })?;

        let t4 = NtpTimestamp::now();

        let response = NtpPacket::decode(&buf[..len]).map_err(|e| AirPlayError::RtspError {
            message: format!("Failed to decode NTP response: {e}"),
            status_code: None,
        })?;

        let t2 = response.receive_timestamp;
        let t3 = response.transmit_timestamp;

        #[allow(
            clippy::cast_possible_wrap,
            reason = "Values represent microseconds which fit safely in i64"
        )]
        let t1_us = t1.to_micros() as i64;
        #[allow(
            clippy::cast_possible_wrap,
            reason = "Values represent microseconds which fit safely in i64"
        )]
        let t2_us = t2.to_micros() as i64;
        #[allow(
            clippy::cast_possible_wrap,
            reason = "Values represent microseconds which fit safely in i64"
        )]
        let t3_us = t3.to_micros() as i64;
        #[allow(
            clippy::cast_possible_wrap,
            reason = "Values represent microseconds which fit safely in i64"
        )]
        let t4_us = t4.to_micros() as i64;

        let offset = ((t2_us - t1_us) + (t3_us - t4_us)) / 2;
        let rtt = (t4.to_micros().saturating_sub(t1.to_micros())).saturating_sub(t3.to_micros().saturating_sub(t2.to_micros()));

        Ok((offset, rtt))
    }
}
