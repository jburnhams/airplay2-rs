//! Playback control module

pub mod playback;
// Other modules will be added in future sections (queue, volume, events)

#[cfg(test)]
mod tests;

pub use playback::{PlaybackController, PlaybackProgress, ShuffleMode};
