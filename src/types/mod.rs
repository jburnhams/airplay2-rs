//! Core types for the airplay2 library

mod config;
mod device;
mod state;
mod track;

#[cfg(test)]
mod tests;

pub use config::{AirPlayConfig, AirPlayConfigBuilder};
pub use device::{AirPlayDevice, DeviceCapabilities};
pub use state::{ConnectionState, PlaybackInfo, PlaybackState, RepeatMode};
pub use track::{QueueItem, TrackInfo};
