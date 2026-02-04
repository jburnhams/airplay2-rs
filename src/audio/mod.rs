//! Audio handling module

#![allow(unused_imports)]
#![allow(dead_code)]

pub mod buffer;
pub mod clock;
pub mod concealment;
pub mod convert;
pub mod format;
pub mod jitter;
pub mod output;
pub mod output_coreaudio;
pub mod output_cpal;
pub mod raop_encoder;

#[cfg(test)]
mod tests;

pub use buffer::AudioRingBuffer;
pub use clock::{AudioClock, TimingSync};
pub use concealment::{Concealer, ConcealmentStrategy};
pub use convert::{convert_channels, convert_samples, from_f32, resample_linear, to_f32};
pub use format::{
    AacProfile, AudioCodec, AudioFormat, ChannelConfig, CodecParams, SampleFormat, SampleRate,
};
pub use jitter::{JitterBuffer, JitterResult, JitterStats, NextPacket};
pub use output::{AudioDevice, AudioOutput, AudioOutputError, OutputState};
