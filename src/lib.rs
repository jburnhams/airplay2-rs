//! # airplay2
//!
//! A pure Rust library for streaming audio to `AirPlay` 2 devices.
//!
//! ## Features
//!
//! - Device discovery via mDNS
//! - `HomeKit` authentication
//! - Audio streaming (PCM and URL-based)
//! - Playback control
//! - Multi-room synchronized playback
//!
//! ## Example
//!
//! ```rust,no_run
//! use std::time::Duration;
//!
//! use airplay2::{AirPlayClient, discover};
//!
//! # async fn example() -> Result<(), airplay2::AirPlayError> {
//! // Discover devices
//! let devices = airplay2::scan(Duration::from_secs(5)).await?;
//!
//! if let Some(device) = devices.first() {
//!     // Connect to device
//!     let client = AirPlayClient::new(airplay2::AirPlayConfig::default());
//!     client.connect(device).await?;
//!
//!     // Stream audio...
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! The library is organized into layers:
//!
//! - **High-level**: `AirPlayPlayer` - Simple, intuitive API
//! - **Mid-level**: `AirPlayClient` - Full control over all features
//! - **Low-level**: Protocol modules - Direct protocol access

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Public modules
/// Error types
pub mod error;
/// State management
pub mod state;
/// Core types
pub mod types;

/// Testing utilities
pub mod testing;

// Internal modules
pub mod audio;
mod client;
pub mod connection;
pub mod control;
pub mod discovery;
mod group;
pub mod net;
mod player;
pub mod protocol;
/// Streaming support
pub mod streaming;

// Re-exports
pub use audio::AudioFormat;
pub use client::AirPlayClient;
pub use control::volume::Volume;
pub use discovery::{DiscoveryEvent, discover, scan};
pub use error::AirPlayError;
pub use group::{DeviceGroup, GroupId, GroupManager};
pub use player::{AirPlayPlayer, PlayerBuilder, quick_connect, quick_connect_to, quick_play};
pub use state::{ClientEvent, ClientState};
pub use types::{
    AirPlayConfig, AirPlayDevice, DeviceCapabilities, PlaybackState, RepeatMode, TrackInfo,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prelude for common imports
///
/// Convenient re-exports
pub mod prelude {
    pub use crate::{
        AirPlayClient, AirPlayConfig, AirPlayDevice, AirPlayError, AirPlayPlayer, AudioFormat,
        PlaybackState, TrackInfo, Volume, discover, quick_connect, quick_connect_to, quick_play,
        scan,
    };
}
