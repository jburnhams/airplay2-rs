//! RAOP timing protocol implementation

use super::packet::RtpDecodeError;
use super::timing::NtpTimestamp;

/// Timing request packet (sent every 3 seconds)
#[derive(Debug, Clone)]
pub struct RaopTimingRequest {
    /// Reference time (when we sent this request)
    pub reference_time: NtpTimestamp,
}

impl RaopTimingRequest {
    /// Packet size
    pub const SIZE: usize = 32;

    /// Create new timing request
    #[must_use]
    pub fn new() -> Self {
        Self {
            reference_time: NtpTimestamp::now(),
        }
    }

    /// Encode to bytes
    #[must_use]
    pub fn encode(&self, sequence: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);

        // RTP header (no SSRC)
        buf.push(0x80); // V=2
        buf.push(0xD2); // M=1, PT=0x52

        buf.extend_from_slice(&sequence.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // Timestamp (unused)

        // Reference time
        buf.extend_from_slice(&self.reference_time.encode());

        // Receive time (0 for request)
        buf.extend_from_slice(&[0u8; 8]);

        // Send time (same as reference for request)
        buf.extend_from_slice(&self.reference_time.encode());

        buf
    }
}

impl Default for RaopTimingRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Timing response packet (from server)
#[derive(Debug, Clone)]
pub struct RaopTimingResponse {
    /// Original reference time (from our request)
    pub reference_time: NtpTimestamp,
    /// Time server received our request
    pub receive_time: NtpTimestamp,
    /// Time server sent this response
    pub send_time: NtpTimestamp,
}

impl RaopTimingResponse {
    /// Decode from bytes
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if buffer is too small
    pub fn decode(buf: &[u8]) -> Result<Self, RtpDecodeError> {
        if buf.len() < 32 {
            return Err(RtpDecodeError::BufferTooSmall {
                needed: 32,
                have: buf.len(),
            });
        }

        // Skip header (8 bytes)
        let reference_time = NtpTimestamp::decode(&buf[8..16]);
        let receive_time = NtpTimestamp::decode(&buf[16..24]);
        let send_time = NtpTimestamp::decode(&buf[24..32]);

        Ok(Self {
            reference_time,
            receive_time,
            send_time,
        })
    }

    /// Calculate clock offset (microseconds)
    ///
    /// offset = ((T2 - T1) + (T3 - T4)) / 2
    #[must_use]
    pub fn calculate_offset(&self, client_receive: NtpTimestamp) -> i64 {
        let t1 = self.reference_time.to_micros() as i64;
        let t2 = self.receive_time.to_micros() as i64;
        let t3 = self.send_time.to_micros() as i64;
        let t4 = client_receive.to_micros() as i64;

        ((t2 - t1) + (t3 - t4)) / 2
    }

    /// Calculate round-trip time (microseconds)
    ///
    /// RTT = (T4 - T1) - (T3 - T2)
    #[must_use]
    pub fn calculate_rtt(&self, client_receive: NtpTimestamp) -> u64 {
        let t1 = self.reference_time.to_micros();
        let t2 = self.receive_time.to_micros();
        let t3 = self.send_time.to_micros();
        let t4 = client_receive.to_micros();

        (t4 - t1).saturating_sub(t3 - t2)
    }
}

/// Timing synchronization manager
#[derive(Default)]
pub struct TimingSync {
    /// Sequence number for timing packets
    sequence: u16,
    /// Current clock offset (microseconds)
    offset: i64,
    /// Current RTT (microseconds)
    rtt: u64,
    /// Number of samples for averaging
    sample_count: u32,
    /// Last timing request sent
    last_request: Option<RaopTimingRequest>,
}

impl TimingSync {
    /// Create new timing sync manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            sequence: 0,
            offset: 0,
            rtt: 0,
            sample_count: 0,
            last_request: None,
        }
    }

    /// Get current clock offset
    #[must_use]
    pub fn offset(&self) -> i64 {
        self.offset
    }

    /// Get current RTT
    #[must_use]
    pub fn rtt(&self) -> u64 {
        self.rtt
    }

    /// Create a timing request packet
    pub fn create_request(&mut self) -> Vec<u8> {
        let request = RaopTimingRequest::new();
        let data = request.encode(self.sequence);
        self.sequence = self.sequence.wrapping_add(1);
        self.last_request = Some(request);
        data
    }

    /// Process timing response
    ///
    /// # Errors
    ///
    /// Returns `RtpDecodeError` if the response buffer is invalid
    pub fn process_response(&mut self, data: &[u8]) -> Result<(), RtpDecodeError> {
        let response = RaopTimingResponse::decode(data)?;
        let receive_time = NtpTimestamp::now();

        let offset = response.calculate_offset(receive_time);
        let rtt = response.calculate_rtt(receive_time);

        // Exponential moving average
        if self.sample_count == 0 {
            self.offset = offset;
            self.rtt = rtt;
        } else {
            // α = 0.125 (1/8) for smoothing
            self.offset = self.offset + (offset - self.offset) / 8;
            self.rtt = self.rtt + (rtt.saturating_sub(self.rtt)) / 8;
        }

        self.sample_count += 1;
        Ok(())
    }

    /// Convert local RTP timestamp to synchronized timestamp
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn local_to_remote(&self, local_ts: u32) -> u32 {
        // Adjust by offset (converted to RTP timestamp units)
        // 44100 samples/sec
        #[allow(clippy::cast_possible_truncation)]
        let offset_samples = (self.offset * 44100 / 1_000_000) as i32;
        (local_ts as i32 + offset_samples) as u32
    }

    /// Convert remote RTP timestamp to local timestamp
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn remote_to_local(&self, remote_ts: u32) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        let offset_samples = (self.offset * 44100 / 1_000_000) as i32;
        (remote_ts as i32 - offset_samples) as u32
    }
}
