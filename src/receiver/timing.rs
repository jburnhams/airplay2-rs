//! Timing synchronization for RAOP receiver
//!
//! Implements NTP-like timing exchange to synchronize clocks
//! between sender and receiver.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::RwLock;

/// Timing packet type for request (0x52)
const TIMING_REQUEST: u8 = 0x52;
/// Timing packet type for response (0x53)
const TIMING_RESPONSE: u8 = 0x53;

/// NTP epoch offset from Unix epoch (seconds from 1900 to 1970)
const NTP_EPOCH_OFFSET: u64 = 2_208_988_800;

/// Timing packet structure
#[derive(Debug, Clone, Copy)]
pub struct TimingPacket {
    /// Packet type (0x52 request, 0x53 response)
    pub packet_type: u8,
    /// Reference timestamp (when we received request, or when sender sent)
    pub reference_time: NtpTimestamp,
    /// Receive timestamp (when we received the packet)
    pub receive_time: NtpTimestamp,
    /// Send timestamp (when we send response)
    pub send_time: NtpTimestamp,
}

/// NTP timestamp (64-bit: 32 seconds + 32 fraction)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NtpTimestamp {
    /// Seconds since NTP epoch (Jan 1, 1900)
    pub seconds: u32,
    /// Fractional part of seconds (1/2^32 resolution)
    pub fraction: u32,
}

impl NtpTimestamp {
    /// Create from current system time
    #[must_use]
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);

        let seconds = now.as_secs() + NTP_EPOCH_OFFSET;
        let nanos = now.subsec_nanos();
        // Convert nanoseconds to NTP fraction (2^32 / 10^9)
        #[allow(
            clippy::cast_possible_truncation,
            reason = "NTP fraction fits in u32"
        )]
        let fraction = ((u64::from(nanos) * 0x1_0000_0000_u64) / 1_000_000_000) as u32;

        Self {
            #[allow(clippy::cast_possible_truncation, reason = "Seconds fit in u32")]
            seconds: seconds as u32,
            fraction,
        }
    }

    /// Create from 64-bit NTP timestamp
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self {
            #[allow(clippy::cast_possible_truncation, reason = "Seconds shift fits in u32")]
            seconds: (value >> 32) as u32,
            #[allow(clippy::cast_possible_truncation, reason = "Fraction fits in u32")]
            fraction: value as u32,
        }
    }

    /// Convert to 64-bit NTP timestamp
    #[must_use]
    pub fn to_u64(&self) -> u64 {
        (u64::from(self.seconds) << 32) | u64::from(self.fraction)
    }

    /// Convert to Duration since NTP epoch
    #[must_use]
    pub fn to_duration(&self) -> Duration {
        let secs = u64::from(self.seconds);
        let nanos = ((u64::from(self.fraction) * 1_000_000_000) / 0x1_0000_0000_u64) as u32;
        Duration::new(secs, nanos)
    }

    /// Difference in microseconds
    #[must_use]
    pub fn diff_micros(&self, other: &Self) -> i64 {
        let self_micros =
            (i64::from(self.seconds) * 1_000_000) + ((i64::from(self.fraction) * 1_000_000) >> 32);
        let other_micros = (i64::from(other.seconds) * 1_000_000)
            + ((i64::from(other.fraction) * 1_000_000) >> 32);
        self_micros - other_micros
    }
}

/// Clock synchronization state
#[derive(Debug)]
pub struct ClockSync {
    /// Computed offset (local - remote) in microseconds
    offset_micros: i64,
    /// Round-trip delay in microseconds
    delay_micros: u64,
    /// Number of sync exchanges
    exchange_count: u32,
    /// Last sync time
    last_sync: Instant,
    /// Moving average of offset
    offset_avg: f64,
    /// Drift rate (microseconds per second)
    #[allow(dead_code)] // Reserved for future use
    drift_rate: f64,
}

impl ClockSync {
    /// Create new clock sync state
    #[must_use]
    pub fn new() -> Self {
        Self {
            offset_micros: 0,
            delay_micros: 0,
            exchange_count: 0,
            last_sync: Instant::now(),
            offset_avg: 0.0,
            drift_rate: 0.0,
        }
    }

    /// Update sync from timing exchange
    ///
    /// NTP-like offset calculation:
    /// offset = ((t2 - t1) + (t3 - t4)) / 2
    /// delay = (t4 - t1) - (t3 - t2)
    ///
    /// Where:
    /// t1 = sender's transmit time
    /// t2 = our receive time
    /// t3 = our transmit time
    /// t4 = sender's receive time (from next sync or estimated)
    pub fn update(
        &mut self,
        sender_transmit: NtpTimestamp, // t1
        our_receive: NtpTimestamp,     // t2
        our_transmit: NtpTimestamp,    // t3
    ) {
        // Simplified: estimate offset as (t2 - t1) - delay/2
        // Without t4, we can only estimate based on previous delay

        let receive_diff = our_receive.diff_micros(&sender_transmit);

        // Update moving average
        let alpha = if self.exchange_count < 10 { 0.5 } else { 0.1 };
        #[allow(
            clippy::cast_precision_loss,
            reason = "Precision loss acceptable for moving average"
        )]
        {
            self.offset_avg = (1.0 - alpha) * self.offset_avg + alpha * (receive_diff as f64);
        }

        // Convert strict casting to avoid clippy warnings if needed, but here simple cast is fine
        // for now
        #[allow(clippy::cast_possible_truncation, reason = "Offset fits in i64")]
        let offset = self.offset_avg as i64;
        self.offset_micros = offset;

        // Estimate delay (simplified)
        let transmit_diff = our_transmit.diff_micros(&our_receive);
        self.delay_micros = transmit_diff.unsigned_abs();

        self.exchange_count += 1;
        self.last_sync = Instant::now();
    }

    /// Get current offset in microseconds
    #[must_use]
    pub fn offset_micros(&self) -> i64 {
        self.offset_micros
    }

    /// Get estimated delay in microseconds
    #[must_use]
    pub fn delay_micros(&self) -> u64 {
        self.delay_micros
    }

    /// Check if sync is stale
    #[must_use]
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.last_sync.elapsed() > max_age
    }
}

impl Default for ClockSync {
    fn default() -> Self {
        Self::new()
    }
}

/// Timing port handler
pub struct TimingHandler {
    socket: Arc<UdpSocket>,
    clock_sync: Arc<RwLock<ClockSync>>,
    #[allow(dead_code)] // Reserved for potential future use or debugging
    sender_addr: Option<SocketAddr>,
}

impl TimingHandler {
    /// Create new timing handler
    #[must_use]
    pub fn new(socket: Arc<UdpSocket>) -> Self {
        Self {
            socket,
            clock_sync: Arc::new(RwLock::new(ClockSync::new())),
            sender_addr: None,
        }
    }

    /// Run timing handler loop
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket access fails.
    pub async fn run(mut self) -> Result<(), std::io::Error> {
        let mut buf = [0u8; 64];

        loop {
            let (len, src) = self.socket.recv_from(&mut buf).await?;

            if len < 32 {
                continue;
            }

            // Remember sender address
            self.sender_addr = Some(src);

            // Parse and respond to timing request
            if let Some(response) = self.handle_timing_packet(&buf[..len]).await {
                self.socket.send_to(&response, src).await?;
            }
        }
    }

    async fn handle_timing_packet(&self, data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 32 {
            return None;
        }

        let packet_type = data[1] & 0x7F;

        if packet_type != TIMING_REQUEST {
            return None;
        }

        // Parse timing request
        // Format:
        // Bytes 0-1: Header (0x80 0x52)
        // Bytes 2-3: Sequence
        // Bytes 4-7: Zero
        // Bytes 8-15: Reference timestamp (zero in request)
        // Bytes 16-23: Receive timestamp (zero in request)
        // Bytes 24-31: Transmit timestamp (sender's send time)

        let sender_transmit = NtpTimestamp::from_u64(u64::from_be_bytes([
            data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
        ]));

        let our_receive = NtpTimestamp::now();

        // Build response
        let our_transmit = NtpTimestamp::now();

        // Update clock sync
        {
            let mut sync = self.clock_sync.write().await;
            sync.update(sender_transmit, our_receive, our_transmit);
        }

        // Build timing response
        let mut response = vec![0u8; 32];
        response[0] = 0x80;
        response[1] = TIMING_RESPONSE | 0x80; // Response type with marker
        response[2] = data[2]; // Echo sequence
        response[3] = data[3];

        // Reference time = sender's transmit time
        let ref_bytes = sender_transmit.to_u64().to_be_bytes();
        response[8..16].copy_from_slice(&ref_bytes);

        // Receive time = when we received request
        let recv_bytes = our_receive.to_u64().to_be_bytes();
        response[16..24].copy_from_slice(&recv_bytes);

        // Transmit time = now
        let send_bytes = our_transmit.to_u64().to_be_bytes();
        response[24..32].copy_from_slice(&send_bytes);

        Some(response)
    }

    /// Get clock sync handle
    #[must_use]
    pub fn clock_sync(&self) -> Arc<RwLock<ClockSync>> {
        self.clock_sync.clone()
    }
}
