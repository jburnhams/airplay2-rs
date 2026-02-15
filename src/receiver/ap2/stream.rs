//! Stream types for `AirPlay` 2 SETUP negotiation

use std::net::SocketAddr;

/// Stream types in SETUP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// General audio stream (type 96)
    Audio,
    /// Control/timing stream (type 103)
    Control,
    /// Event channel (type 130)
    Event,
    /// Timing (PTP) stream (type 150)
    Timing,
    /// Buffered audio (type 96 with buffered flag)
    BufferedAudio,
    /// Unknown stream type
    Unknown(u32),
}

impl From<u32> for StreamType {
    fn from(value: u32) -> Self {
        match value {
            96 => Self::Audio,
            103 => Self::Control,
            130 => Self::Event,
            150 => Self::Timing,
            _ => Self::Unknown(value),
        }
    }
}

impl From<StreamType> for i64 {
    fn from(val: StreamType) -> Self {
        match val {
            StreamType::Audio | StreamType::BufferedAudio => 96,
            StreamType::Control => 103,
            StreamType::Event => 130,
            StreamType::Timing => 150,
            StreamType::Unknown(t) => i64::from(t),
        }
    }
}

/// Timing protocol selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingProtocol {
    /// Network Time Protocol (legacy)
    #[default]
    Ntp,
    /// Precision Time Protocol (`AirPlay` 2)
    Ptp,
    /// No timing (not recommended)
    None,
}

impl From<&str> for TimingProtocol {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PTP" => Self::Ptp,
            "NONE" => Self::None,
            _ => Self::Ntp,
        }
    }
}

/// Encryption type for audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionType {
    /// No encryption
    #[default]
    None,
    /// `AirPlay` 1 style (AES-128-CTR)
    Aes128Ctr,
    /// `AirPlay` 2 style (ChaCha20-Poly1305)
    ChaCha20Poly1305,
}

/// Timing peer information for PTP
#[derive(Debug, Clone)]
pub struct TimingPeerInfo {
    /// Peer ID
    pub peer_id: u64,
    /// Peer addresses
    pub addresses: Vec<SocketAddr>,
}

/// Audio stream format parameters
#[derive(Debug, Clone)]
pub struct AudioStreamFormat {
    /// Codec type (96=ALAC, 97=AAC, etc.)
    pub codec: u32,
    /// Sample rate (Hz)
    pub sample_rate: u32,
    /// Channels
    pub channels: u8,
    /// Bits per sample
    pub bits_per_sample: u8,
    /// Frames per packet
    pub frames_per_packet: u32,
    /// Compression type (for ALAC)
    pub compression_type: Option<u32>,
    /// Spf (samples per frame)
    pub spf: Option<u32>,
}
