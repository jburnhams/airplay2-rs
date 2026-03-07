use super::timing::NtpTimestamp;
use crate::error::AirPlayError;
use std::time::Duration;
use tokio::net::UdpSocket;

/// Standard NTP request packet size
const NTP_PACKET_SIZE: usize = 48;

/// NTP Client for standard RFC 5905 timing sync
pub struct NtpClient {
    /// Remote NTP server address
    server_addr: String,
    /// Timeout for requests
    timeout: Duration,
}

impl NtpClient {
    /// Create new NTP client
    #[must_use]
    pub fn new(server_addr: String, timeout: Duration) -> Self {
        Self {
            server_addr,
            timeout,
        }
    }

    /// Perform NTP timing exchange
    ///
    /// # Errors
    ///
    /// Returns an error if networking or decoding fails.
    pub async fn get_offset(&self) -> Result<i64, AirPlayError> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(AirPlayError::NetworkError)?;

        // Format packet: standard NTP v4 client request
        let mut req = [0u8; NTP_PACKET_SIZE];
        req[0] = 0x1B; // LI=0, VN=3, Mode=3 (Client)

        let t1 = NtpTimestamp::now();
        let t1_bytes = t1.encode();
        req[40..48].copy_from_slice(&t1_bytes);

        tokio::time::timeout(self.timeout, socket.send_to(&req, &self.server_addr))
            .await
            .map_err(|_| AirPlayError::Timeout)?
            .map_err(AirPlayError::NetworkError)?;

        let mut buf = [0u8; NTP_PACKET_SIZE];
        let (len, _) = tokio::time::timeout(self.timeout, socket.recv_from(&mut buf))
            .await
            .map_err(|_| AirPlayError::Timeout)?
            .map_err(AirPlayError::NetworkError)?;

        if len < NTP_PACKET_SIZE {
            return Err(AirPlayError::CodecError {
                message: format!("Invalid NTP response size: {len}"),
            });
        }

        let t4 = NtpTimestamp::now();
        let t2 = NtpTimestamp::decode(&buf[32..40]);
        let t3 = NtpTimestamp::decode(&buf[40..48]);

        #[allow(clippy::cast_possible_wrap, reason = "NTP micros fit in i64")]
        let t1_micros = t1.to_micros() as i64;
        #[allow(clippy::cast_possible_wrap, reason = "NTP micros fit in i64")]
        let t2_micros = t2.to_micros() as i64;
        #[allow(clippy::cast_possible_wrap, reason = "NTP micros fit in i64")]
        let t3_micros = t3.to_micros() as i64;
        #[allow(clippy::cast_possible_wrap, reason = "NTP micros fit in i64")]
        let t4_micros = t4.to_micros() as i64;

        let offset = ((t2_micros - t1_micros) + (t3_micros - t4_micros)) / 2;
        Ok(offset)
    }
}
