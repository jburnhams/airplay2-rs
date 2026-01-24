//! Core types for the airplay2 library

mod config;
mod device;
/// RAOP (AirPlay 1) types
pub mod raop;
mod state;
mod track;

#[cfg(test)]
mod tests;

pub use config::{AirPlayConfig, AirPlayConfigBuilder};
pub use device::{AirPlayDevice, DeviceCapabilities};
pub use raop::{RaopCapabilities, RaopCodec, RaopEncryption, RaopMetadataType};
pub use state::{ConnectionState, PlaybackInfo, PlaybackState, RepeatMode};
pub use track::{QueueItem, QueueItemId, TrackInfo};
