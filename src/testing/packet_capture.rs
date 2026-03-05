//! Packet Capture Replay for Testing
//!
//! Allows replaying captured `AirPlay` traffic for testing.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

/// Captured packet
#[derive(Debug, Clone)]
pub struct CapturedPacket {
    /// Timestamp offset from start (microseconds)
    pub timestamp_us: u64,
    /// Direction (true = sender -> receiver)
    pub inbound: bool,
    /// Protocol (TCP, UDP)
    pub protocol: CaptureProtocol,
    /// Packet data
    pub data: Vec<u8>,
}

/// Network protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProtocol {
    /// TCP protocol
    Tcp,
    /// UDP protocol
    Udp,
}

/// Capture file loader
pub struct CaptureLoader;

fn decode_hex(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

impl CaptureLoader {
    /// Load capture from hex dump file
    ///
    /// Format: `timestamp_us direction protocol hex_data`
    ///
    /// # Errors
    /// Returns `CaptureError` if file cannot be read or parsed.
    pub fn load_hex_dump(path: &Path) -> Result<Vec<CapturedPacket>, CaptureError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut packets = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            let timestamp_us: u64 = parts[0].parse().map_err(|_| CaptureError::InvalidFormat)?;
            let inbound = parts[1] == "IN";
            let protocol = match parts[2] {
                "TCP" => CaptureProtocol::Tcp,
                "UDP" => CaptureProtocol::Udp,
                _ => continue,
            };
            let data = decode_hex(parts[3]).map_err(|_| CaptureError::InvalidHex)?;

            packets.push(CapturedPacket {
                timestamp_us,
                inbound,
                protocol,
                data,
            });
        }

        Ok(packets)
    }

    /// Load capture from pcap file (simplified)
    ///
    /// # Errors
    /// Returns `CaptureError::UnsupportedFormat`.
    pub fn load_pcap(_path: &Path) -> Result<Vec<CapturedPacket>, CaptureError> {
        // Would use pcap crate for real implementation
        Err(CaptureError::UnsupportedFormat)
    }
}

/// Capture replay engine
pub struct CaptureReplay {
    packets: Vec<CapturedPacket>,
    current_index: usize,
    start_time: Option<std::time::Instant>,
    base_timestamp_us: Option<u64>,
}

impl CaptureReplay {
    /// Create a new capture replay engine.
    #[must_use]
    pub fn new(packets: Vec<CapturedPacket>) -> Self {
        Self {
            packets,
            current_index: 0,
            start_time: None,
            base_timestamp_us: None,
        }
    }

    /// Get next inbound packet (sender -> receiver)
    pub fn next_inbound(&mut self) -> Option<&CapturedPacket> {
        while self.current_index < self.packets.len() {
            let packet = &self.packets[self.current_index];
            self.current_index += 1;
            if packet.inbound {
                return Some(packet);
            }
        }
        None
    }

    /// Get next packet with timing
    ///
    /// # Panics
    /// Panics if `target < elapsed`.
    pub async fn next_timed(&mut self) -> Option<&CapturedPacket> {
        if self.current_index >= self.packets.len() {
            return None;
        }

        let packet = &self.packets[self.current_index];

        // Wait for correct time
        if let Some(start) = self.start_time {
            if let Some(base) = self.base_timestamp_us {
                let relative_target_us = packet.timestamp_us.saturating_sub(base);
                let target = Duration::from_micros(relative_target_us);
                let elapsed = start.elapsed();
                if target > elapsed {
                    tokio::time::sleep(target.checked_sub(elapsed).unwrap()).await;
                }
            }
        } else {
            self.start_time = Some(std::time::Instant::now());
            self.base_timestamp_us = Some(packet.timestamp_us);
        }

        self.current_index += 1;
        Some(packet)
    }

    /// Reset replay
    pub fn reset(&mut self) {
        self.current_index = 0;
        self.start_time = None;
        self.base_timestamp_us = None;
    }
}

/// Errors from capture loading
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid format
    #[error("Invalid capture format")]
    InvalidFormat,

    /// Invalid hex data
    #[error("Invalid hex data")]
    InvalidHex,

    /// Unsupported format
    #[error("Unsupported capture format")]
    UnsupportedFormat,
}
