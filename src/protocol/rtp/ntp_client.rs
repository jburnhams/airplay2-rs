use std::time::Duration;

use tokio::net::UdpSocket;

use super::timing::NtpTimestamp;
use crate::error::AirPlayError;

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

        // Resolve server_addr to a single IP before sending
        // This ensures the response peer_addr matches exactly where we sent the packet
        let mut addrs = tokio::net::lookup_host(&self.server_addr)
            .await
            .map_err(|_| {
                AirPlayError::NetworkError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Failed to resolve NTP server address",
                ))
            })?;

        let target_addr = addrs.next().ok_or_else(|| {
            AirPlayError::NetworkError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No addresses found for NTP server",
            ))
        })?;

        // Format packet: standard NTP v4 client request
        let mut req = [0u8; NTP_PACKET_SIZE];
        req[0] = 0x23; // LI=0, VN=4, Mode=3 (Client)

        let t1 = NtpTimestamp::now();
        let t1_bytes = t1.encode();
        req[40..48].copy_from_slice(&t1_bytes);

        tokio::time::timeout(self.timeout, socket.send_to(&req, target_addr))
            .await
            .map_err(|_| AirPlayError::Timeout)?
            .map_err(AirPlayError::NetworkError)?;

        let mut buf = [0u8; NTP_PACKET_SIZE];

        let end_time = std::time::Instant::now() + self.timeout;
        let mut valid_response = false;

        // Loop until a valid response is received or timeout occurs
        while std::time::Instant::now() < end_time {
            let remaining = end_time - std::time::Instant::now();
            let result = tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await;

            match result {
                Ok(Ok((bytes_read, peer_addr))) => {
                    // Verify IP matches
                    if peer_addr != target_addr {
                        continue;
                    }

                    if bytes_read < NTP_PACKET_SIZE {
                        continue;
                    }

                    // Verify Origin Timestamp matches what we sent
                    let origin_ts_bytes = &buf[24..32];
                    if origin_ts_bytes != t1_bytes {
                        continue;
                    }

                    valid_response = true;
                    break;
                }
                Ok(Err(e)) => {
                    tracing::debug!("NTP socket error: {}", e);
                }
                Err(_) => return Err(AirPlayError::Timeout), // Timeout
            }
        }

        if !valid_response {
            return Err(AirPlayError::Timeout);
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
