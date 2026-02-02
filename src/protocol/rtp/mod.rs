//! RTP/RAOP protocol implementation for AirPlay audio streaming

#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(missing_docs)]

mod codec;
mod control;
mod packet;
pub mod packet_buffer;
pub mod raop;
pub mod raop_timing;
mod timing;

#[cfg(test)]
mod codec_tests;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod packet_buffer_tests;
#[cfg(test)]
mod packet_tests;
#[cfg(test)]
mod raop_tests;
#[cfg(test)]
mod timing_tests;
#[cfg(test)]
mod wrapping_tests;

pub use codec::{AudioPacketBuilder, RtpCodec, RtpCodecError, RtpEncryptionMode};
pub use control::{ControlPacket, RetransmitRequest};
pub use packet::{PayloadType, RtpDecodeError, RtpHeader, RtpPacket};
pub use timing::{NtpTimestamp, TimingPacket, TimingRequest, TimingResponse};

/// RTP protocol constants for AirPlay
pub mod constants {
    /// Default RTP audio port
    pub const AUDIO_PORT: u16 = 6000;
    /// Default RTP control port
    pub const CONTROL_PORT: u16 = 6001;
    /// Default RTP timing port
    pub const TIMING_PORT: u16 = 6002;

    /// Audio frames per RTP packet (352 samples at 44.1kHz ≈ 8ms)
    pub const FRAMES_PER_PACKET: usize = 352;

    /// Audio sample rate
    pub const SAMPLE_RATE: u32 = 44100;

    /// Audio channels (stereo)
    pub const CHANNELS: u8 = 2;

    /// Bits per sample
    pub const BITS_PER_SAMPLE: u8 = 16;
}
