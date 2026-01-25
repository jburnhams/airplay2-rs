# Section 40: Timing Synchronization

## Dependencies
- **Section 39**: RTP Receiver Core (timing port handling)
- **Section 37**: Session Management (socket allocation)
- **Section 06**: RTP Protocol (timing packet structures)

## Overview

This section implements NTP-like timing synchronization for the AirPlay receiver. Accurate timing is critical for:

1. **Buffer management**: Knowing when to start playback
2. **Sync packets**: Correlating RTP timestamps to wall-clock time
3. **Multi-room sync**: If extended to support grouped playback
4. **Latency measurement**: Understanding sender-receiver delay

The timing protocol exchanges packets on the timing UDP port, allowing both sender and receiver to compute clock offsets and network delay.

## Objectives

- Implement timing request/response handler
- Compute clock offset from NTP-like exchange
- Map RTP timestamps to local playback time
- Track and compensate for clock drift
- Provide accurate "now playing" timestamps

---

## Tasks

### 40.1 Timing Packet Handler

- [ ] **40.1.1** Implement timing request/response on timing port

**File:** `src/receiver/timing.rs`

```rust
//! Timing synchronization for RAOP receiver
//!
//! Implements NTP-like timing exchange to synchronize clocks
//! between sender and receiver.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

/// Timing packet types
const TIMING_REQUEST: u8 = 0x52;
const TIMING_RESPONSE: u8 = 0x53;

/// NTP epoch offset from Unix epoch (seconds from 1900 to 1970)
const NTP_EPOCH_OFFSET: u64 = 2208988800;

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
#[derive(Debug, Clone, Copy, Default)]
pub struct NtpTimestamp {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTimestamp {
    /// Create from current system time
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);

        let seconds = now.as_secs() + NTP_EPOCH_OFFSET;
        let nanos = now.subsec_nanos();
        // Convert nanoseconds to NTP fraction (2^32 / 10^9)
        let fraction = ((nanos as u64 * 0x100000000u64) / 1_000_000_000) as u32;

        Self {
            seconds: seconds as u32,
            fraction,
        }
    }

    /// Create from 64-bit NTP timestamp
    pub fn from_u64(value: u64) -> Self {
        Self {
            seconds: (value >> 32) as u32,
            fraction: value as u32,
        }
    }

    /// Convert to 64-bit NTP timestamp
    pub fn to_u64(&self) -> u64 {
        ((self.seconds as u64) << 32) | (self.fraction as u64)
    }

    /// Convert to Duration since NTP epoch
    pub fn to_duration(&self) -> Duration {
        let secs = self.seconds as u64;
        let nanos = ((self.fraction as u64 * 1_000_000_000) / 0x100000000u64) as u32;
        Duration::new(secs, nanos)
    }

    /// Difference in microseconds
    pub fn diff_micros(&self, other: &Self) -> i64 {
        let self_micros = (self.seconds as i64 * 1_000_000) +
            ((self.fraction as i64 * 1_000_000) >> 32);
        let other_micros = (other.seconds as i64 * 1_000_000) +
            ((other.fraction as i64 * 1_000_000) >> 32);
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
    drift_rate: f64,
}

impl ClockSync {
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
        sender_transmit: NtpTimestamp,   // t1
        our_receive: NtpTimestamp,       // t2
        our_transmit: NtpTimestamp,      // t3
    ) {
        // Simplified: estimate offset as (t2 - t1) - delay/2
        // Without t4, we can only estimate based on previous delay

        let receive_diff = our_receive.diff_micros(&sender_transmit);

        // Update moving average
        let alpha = if self.exchange_count < 10 { 0.5 } else { 0.1 };
        self.offset_avg = (1.0 - alpha) * self.offset_avg + alpha * (receive_diff as f64);
        self.offset_micros = self.offset_avg as i64;

        // Estimate delay (simplified)
        let transmit_diff = our_transmit.diff_micros(&our_receive);
        self.delay_micros = transmit_diff.unsigned_abs();

        self.exchange_count += 1;
        self.last_sync = Instant::now();
    }

    /// Get current offset in microseconds
    pub fn offset_micros(&self) -> i64 {
        self.offset_micros
    }

    /// Get estimated delay in microseconds
    pub fn delay_micros(&self) -> u64 {
        self.delay_micros
    }

    /// Check if sync is stale
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
    sender_addr: Option<SocketAddr>,
}

impl TimingHandler {
    pub fn new(socket: Arc<UdpSocket>) -> Self {
        Self {
            socket,
            clock_sync: Arc::new(RwLock::new(ClockSync::new())),
            sender_addr: None,
        }
    }

    /// Run timing handler loop
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

        let sender_transmit = NtpTimestamp::from_u64(
            u64::from_be_bytes([
                data[24], data[25], data[26], data[27],
                data[28], data[29], data[30], data[31],
            ])
        );

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
        response[1] = TIMING_RESPONSE | 0x80;  // Response type with marker
        response[2] = data[2];  // Echo sequence
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
    pub fn clock_sync(&self) -> Arc<RwLock<ClockSync>> {
        self.clock_sync.clone()
    }
}
```

---

### 40.2 RTP to Wall-Clock Mapping

- [ ] **40.2.1** Map RTP timestamps to playback time

**File:** `src/receiver/playback_timing.rs`

```rust
//! RTP timestamp to playback time mapping

use super::timing::{ClockSync, NtpTimestamp};
use super::control_receiver::SyncPacket;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Maps RTP timestamps to wall-clock time for playback scheduling
pub struct PlaybackTiming {
    /// Sample rate (typically 44100)
    sample_rate: u32,
    /// Reference RTP timestamp (from sync packet)
    ref_rtp_timestamp: Option<u32>,
    /// Reference NTP timestamp (from sync packet)
    ref_ntp_timestamp: Option<NtpTimestamp>,
    /// Reference local time
    ref_local_time: Option<Instant>,
    /// Clock sync for offset
    clock_sync: Arc<RwLock<ClockSync>>,
    /// Target latency in samples
    target_latency_samples: u32,
}

impl PlaybackTiming {
    pub fn new(sample_rate: u32, clock_sync: Arc<RwLock<ClockSync>>) -> Self {
        Self {
            sample_rate,
            ref_rtp_timestamp: None,
            ref_ntp_timestamp: None,
            ref_local_time: None,
            clock_sync,
            // Default 2 second latency
            target_latency_samples: sample_rate * 2,
        }
    }

    /// Update reference from sync packet
    pub fn update_from_sync(&mut self, sync: &SyncPacket) {
        self.ref_rtp_timestamp = Some(sync.rtp_timestamp_at_ntp);
        self.ref_ntp_timestamp = Some(NtpTimestamp::from_u64(sync.ntp_timestamp));
        self.ref_local_time = Some(Instant::now());

        tracing::debug!(
            "Sync update: RTP {} at NTP {}",
            sync.rtp_timestamp_at_ntp,
            sync.ntp_timestamp
        );
    }

    /// Set target latency
    pub fn set_target_latency(&mut self, samples: u32) {
        self.target_latency_samples = samples;
    }

    /// Get target latency in duration
    pub fn target_latency(&self) -> Duration {
        Duration::from_secs_f64(
            self.target_latency_samples as f64 / self.sample_rate as f64
        )
    }

    /// Calculate when an RTP timestamp should be played
    ///
    /// Returns the Instant at which the audio with this RTP timestamp
    /// should be sent to the audio device.
    pub async fn playback_time(&self, rtp_timestamp: u32) -> Option<Instant> {
        let ref_rtp = self.ref_rtp_timestamp?;
        let ref_local = self.ref_local_time?;

        // Calculate samples since reference
        let samples_diff = rtp_timestamp.wrapping_sub(ref_rtp) as i64;

        // Convert to duration
        let time_diff = Duration::from_secs_f64(
            samples_diff as f64 / self.sample_rate as f64
        );

        // Add target latency
        let latency = self.target_latency();

        // Calculate playback time
        let playback_time = if samples_diff >= 0 {
            ref_local + time_diff + latency
        } else {
            // Past timestamp
            ref_local.checked_sub(Duration::from_secs_f64(
                (-samples_diff) as f64 / self.sample_rate as f64
            ))? + latency
        };

        Some(playback_time)
    }

    /// Check if an RTP timestamp is ready for playback
    pub async fn is_ready_for_playback(&self, rtp_timestamp: u32) -> bool {
        if let Some(playback_time) = self.playback_time(rtp_timestamp).await {
            Instant::now() >= playback_time
        } else {
            // No sync yet, use simple delay
            false
        }
    }

    /// Get delay until playback time
    pub async fn delay_until_playback(&self, rtp_timestamp: u32) -> Option<Duration> {
        let playback_time = self.playback_time(rtp_timestamp).await?;
        let now = Instant::now();

        if now >= playback_time {
            Some(Duration::ZERO)
        } else {
            Some(playback_time - now)
        }
    }

    /// Convert RTP timestamp difference to duration
    pub fn rtp_to_duration(&self, rtp_samples: u32) -> Duration {
        Duration::from_secs_f64(rtp_samples as f64 / self.sample_rate as f64)
    }
}
```

---

## Unit Tests

### 40.3 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntp_timestamp_now() {
        let ts = NtpTimestamp::now();

        // Should be after year 2020 in NTP time
        // 2020 in NTP = 3786825600 (seconds since 1900)
        assert!(ts.seconds > 3786825600);
    }

    #[test]
    fn test_ntp_timestamp_roundtrip() {
        let original = NtpTimestamp {
            seconds: 12345678,
            fraction: 0xABCDEF00,
        };

        let u64_val = original.to_u64();
        let restored = NtpTimestamp::from_u64(u64_val);

        assert_eq!(original.seconds, restored.seconds);
        assert_eq!(original.fraction, restored.fraction);
    }

    #[test]
    fn test_ntp_diff_micros() {
        let t1 = NtpTimestamp { seconds: 1000, fraction: 0 };
        let t2 = NtpTimestamp { seconds: 1001, fraction: 0 };

        let diff = t2.diff_micros(&t1);
        assert_eq!(diff, 1_000_000);  // 1 second = 1,000,000 microseconds
    }

    #[test]
    fn test_clock_sync_update() {
        let mut sync = ClockSync::new();

        let sender = NtpTimestamp { seconds: 1000, fraction: 0 };
        let receive = NtpTimestamp { seconds: 1000, fraction: 0x80000000 };  // +0.5s
        let transmit = NtpTimestamp { seconds: 1000, fraction: 0x80000001 };

        sync.update(sender, receive, transmit);

        assert!(sync.exchange_count == 1);
        assert!(sync.offset_micros() != 0 || sync.delay_micros() != 0);
    }

    #[tokio::test]
    async fn test_playback_timing() {
        let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
        let mut timing = PlaybackTiming::new(44100, clock_sync);

        // Set reference
        let sync = SyncPacket {
            extension: false,
            rtp_timestamp: 44100,
            ntp_timestamp: NtpTimestamp::now().to_u64(),
            rtp_timestamp_at_ntp: 44100,
        };
        timing.update_from_sync(&sync);

        // Timestamp one second later should play ~1 second later
        let playback = timing.playback_time(44100 + 44100).await;
        assert!(playback.is_some());
    }

    #[test]
    fn test_rtp_to_duration() {
        let clock_sync = Arc::new(RwLock::new(ClockSync::new()));
        let timing = PlaybackTiming::new(44100, clock_sync);

        let duration = timing.rtp_to_duration(44100);
        assert!((duration.as_secs_f64() - 1.0).abs() < 0.001);

        let duration = timing.rtp_to_duration(22050);
        assert!((duration.as_secs_f64() - 0.5).abs() < 0.001);
    }
}
```

---

## Acceptance Criteria

- [ ] Receive and respond to timing requests
- [ ] Compute clock offset from timing exchanges
- [ ] Track round-trip delay
- [ ] Map RTP timestamps to local playback time
- [ ] Handle sync packet updates
- [ ] Support configurable target latency
- [ ] Detect stale synchronization
- [ ] All unit tests pass

---

## Notes

- **NTP format**: 64-bit timestamp with 32 bits seconds, 32 bits fraction
- **Offset calculation**: Simplified without full NTP algorithm
- **Drift compensation**: Not yet implemented; future enhancement
- **Sync frequency**: Sender typically sends sync packets every few seconds
- **Latency**: 2 seconds default matches shairport-sync

---

## References

- [RFC 5905](https://tools.ietf.org/html/rfc5905) - Network Time Protocol Version 4
- [RAOP Timing Protocol](https://nto.github.io/AirPlay.html#audio-timing)
