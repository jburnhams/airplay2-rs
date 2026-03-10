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

/// Capture protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProtocol {
    /// TCP
    Tcp,
    /// UDP
    Udp,
}

/// Capture file loader
pub struct CaptureLoader;

impl CaptureLoader {
    /// Load capture from hex dump file
    ///
    /// Format: `timestamp_us direction protocol hex_data`
    ///
    /// # Errors
    /// Returns `CaptureError` if the file cannot be read or parsed.
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
            let data = hex::decode(parts[3]).map_err(|_| CaptureError::InvalidHex)?;

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
    /// Returns `CaptureError` if the file cannot be read or parsed.
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
}

impl CaptureReplay {
    /// Creates a new capture replay
    #[must_use]
    pub fn new(packets: Vec<CapturedPacket>) -> Self {
        Self {
            packets,
            current_index: 0,
            start_time: None,
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
    pub async fn next_timed(&mut self) -> Option<&CapturedPacket> {
        if self.current_index >= self.packets.len() {
            return None;
        }

        let packet = &self.packets[self.current_index];

        // Wait for correct time
        if let Some(start) = self.start_time {
            let target = Duration::from_micros(packet.timestamp_us);
            let elapsed = start.elapsed();
            if target > elapsed {
                tokio::time::sleep(
                    #[allow(clippy::unchecked_time_subtraction, reason = "Checked above")]
                    (target - elapsed),
                )
                .await;
            }
        } else {
            self.start_time = Some(std::time::Instant::now());
        }

        self.current_index += 1;
        Some(packet)
    }

    /// Reset replay
    pub fn reset(&mut self) {
        self.current_index = 0;
        self.start_time = None;
    }
}

/// Capture error
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid format
    #[error("Invalid capture format")]
    InvalidFormat,

    /// Invalid hex
    #[error("Invalid hex data")]
    InvalidHex,

    /// Unsupported format
    #[error("Unsupported capture format")]
    UnsupportedFormat,
}

// **DO NOT MOVE THIS MODULE:** These tests live inside the module to
// verify internal test structs natively without making them `pub`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_replay() {
        let packets = vec![
            CapturedPacket {
                timestamp_us: 0,
                inbound: true,
                protocol: CaptureProtocol::Tcp,
                data: vec![1, 2, 3],
            },
            CapturedPacket {
                timestamp_us: 1000,
                inbound: false,
                protocol: CaptureProtocol::Tcp,
                data: vec![4, 5, 6],
            },
            CapturedPacket {
                timestamp_us: 2000,
                inbound: true,
                protocol: CaptureProtocol::Tcp,
                data: vec![7, 8, 9],
            },
        ];

        let mut replay = CaptureReplay::new(packets);

        // Should get inbound packets only
        let p1 = replay.next_inbound().unwrap();
        assert_eq!(p1.data, vec![1, 2, 3]);

        let p2 = replay.next_inbound().unwrap();
        assert_eq!(p2.data, vec![7, 8, 9]);

        assert!(replay.next_inbound().is_none());
    }
}
