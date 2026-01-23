//! Audio handling module

#![allow(unused_imports)]
#![allow(dead_code)]

pub mod convert;
pub mod format;

#[cfg(test)]
mod tests;

pub use convert::{convert_channels, convert_samples, from_f32, resample_linear, to_f32};
pub use format::{
    AacProfile, AudioCodec, AudioFormat, ChannelConfig, CodecParams, SampleFormat, SampleRate,
};
