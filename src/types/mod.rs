//! Core types for the airplay2 library

mod config;
mod device;
mod state;
mod track;

pub use config::{AirPlayConfig, AirPlayConfigBuilder};
pub use device::{AirPlayDevice, DeviceCapabilities};
pub use state::{ConnectionState, PlaybackInfo, PlaybackState, RepeatMode};
pub use track::{QueueItem, TrackInfo};
