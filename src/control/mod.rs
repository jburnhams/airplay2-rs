//! Playback control module

pub mod playback;
pub mod queue;
// Other modules will be added in future sections (volume, events)

#[cfg(test)]
mod tests;

pub use playback::{PlaybackController, PlaybackProgress, ShuffleMode};
pub use queue::PlaybackQueue;
