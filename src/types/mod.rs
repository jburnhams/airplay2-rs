//! Core types for the airplay2 library

mod device;
mod track;
mod state;
mod config;

pub use device::{AirPlayDevice, DeviceCapabilities};
pub use track::{TrackInfo, QueueItem};
pub use state::{PlaybackState, PlaybackInfo, RepeatMode, ConnectionState};
pub use config::{AirPlayConfig, AirPlayConfigBuilder};
