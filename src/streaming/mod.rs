//! Audio streaming

mod pcm;
mod source;

#[cfg(test)]
mod tests;

pub use pcm::{PcmStreamer, StreamerState};
pub use source::{AudioSource, CallbackSource, SilenceSource, SliceSource};
