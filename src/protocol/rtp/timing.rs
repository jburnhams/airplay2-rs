/// NTP timestamp (64-bit, seconds since 1900-01-01)
#[derive(Debug, Clone, Copy, Default)]
pub struct NtpTimestamp {
    /// Seconds since NTP epoch
    pub seconds: u32,
    /// Fractional seconds (1/2^32 of a second)
    pub fraction: u32,
}

impl NtpTimestamp {
    /// NTP epoch offset from Unix epoch (70 years in seconds)
    const NTP_UNIX_OFFSET: u64 = 2208988800;

    /// Create from current time
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let ntp_secs = duration.as_secs() + Self::NTP_UNIX_OFFSET;
        let fraction = ((duration.subsec_nanos() as u64) << 32) / 1_000_000_000;

        Self {
            seconds: ntp_secs as u32,
            fraction: fraction as u32,
        }
    }

    /// Encode to 8 bytes
    pub fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        buf[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        buf
    }

    /// Decode from 8 bytes
    pub fn decode(buf: &[u8]) -> Self {
        Self {
            seconds: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            fraction: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        }
    }

    /// Convert to microseconds since NTP epoch
    pub fn to_micros(&self) -> u64 {
        let secs = self.seconds as u64;
        let frac_micros = ((self.fraction as u64) * 1_000_000) >> 32;
        secs * 1_000_000 + frac_micros
    }
}

/// Timing request packet
#[derive(Debug, Clone)]
pub struct TimingRequest {
    /// Reference timestamp
    pub reference_time: NtpTimestamp,
    /// Receive timestamp (zero in request)
    pub receive_time: NtpTimestamp,
    /// Send timestamp
    pub send_time: NtpTimestamp,
}

impl TimingRequest {
    /// Packet size
    pub const SIZE: usize = 40;

    /// Create a new timing request
    pub fn new() -> Self {
        let now = NtpTimestamp::now();
        Self {
            reference_time: now,
            receive_time: NtpTimestamp::default(),
            send_time: now,
        }
    }

    /// Encode to bytes (including RTP header)
    pub fn encode(&self, sequence: u16, ssrc: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32);

        // RTP header for timing request
        buf.push(0x80); // V=2, P=0, X=0, CC=0
        buf.push(0xD2); // M=1, PT=0x52
        buf.extend_from_slice(&sequence.to_be_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Timestamp (not used)
        buf.extend_from_slice(&ssrc.to_be_bytes());

        // Timing data
        buf.extend_from_slice(&[0u8; 4]); // Padding
        buf.extend_from_slice(&self.reference_time.encode());
        buf.extend_from_slice(&self.receive_time.encode());
        buf.extend_from_slice(&self.send_time.encode());

        buf
    }
}

/// Timing response packet
#[derive(Debug, Clone)]
pub struct TimingResponse {
    /// Original reference timestamp (from request)
    pub reference_time: NtpTimestamp,
    /// Time server received request
    pub receive_time: NtpTimestamp,
    /// Time server sent response
    pub send_time: NtpTimestamp,
}

impl TimingResponse {
    /// Decode from bytes (excluding RTP header)
    pub fn decode(buf: &[u8]) -> Result<Self, super::packet::RtpDecodeError> {
        if buf.len() < 24 {
            return Err(super::packet::RtpDecodeError::BufferTooSmall {
                needed: 24,
                have: buf.len(),
            });
        }

        Ok(Self {
            reference_time: NtpTimestamp::decode(&buf[0..8]),
            receive_time: NtpTimestamp::decode(&buf[8..16]),
            send_time: NtpTimestamp::decode(&buf[16..24]),
        })
    }

    /// Calculate clock offset (server time - client time)
    ///
    /// Returns offset in microseconds
    pub fn calculate_offset(&self, client_receive_time: NtpTimestamp) -> i64 {
        // offset = ((T2 - T1) + (T3 - T4)) / 2
        // where:
        // T1 = reference_time (client send)
        // T2 = receive_time (server receive)
        // T3 = send_time (server send)
        // T4 = client_receive_time

        let t1 = self.reference_time.to_micros() as i64;
        let t2 = self.receive_time.to_micros() as i64;
        let t3 = self.send_time.to_micros() as i64;
        let t4 = client_receive_time.to_micros() as i64;

        ((t2 - t1) + (t3 - t4)) / 2
    }

    /// Calculate round-trip time
    ///
    /// Returns RTT in microseconds
    pub fn calculate_rtt(&self, client_receive_time: NtpTimestamp) -> u64 {
        // RTT = (T4 - T1) - (T3 - T2)

        let t1 = self.reference_time.to_micros();
        let t2 = self.receive_time.to_micros();
        let t3 = self.send_time.to_micros();
        let t4 = client_receive_time.to_micros();

        (t4 - t1).saturating_sub(t3 - t2)
    }
}

/// Timing packet (request or response)
#[derive(Debug, Clone)]
pub enum TimingPacket {
    Request(TimingRequest),
    Response(TimingResponse),
}
