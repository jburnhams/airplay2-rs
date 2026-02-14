//! Audio streaming

/// File-based audio source (requires `decoders` feature)
#[cfg(feature = "decoders")]
pub mod file;
mod pcm;
pub mod raop_streamer;
mod resampler;
pub mod source;
mod url;

#[cfg(test)]
mod tests;

pub use pcm::{PcmStreamer, RtpSender, StreamerState};
pub use raop_streamer::{RaopStreamConfig, RaopStreamer};
pub use resampler::ResamplingSource;
pub use source::{AudioSource, CallbackSource, SilenceSource, SliceSource};
pub use url::{PlaybackInfo, UrlStreamer};
