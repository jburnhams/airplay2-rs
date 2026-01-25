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
//! use airplay2::{discover, AirPlayClient};
//! use std::time::Duration;
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
pub use client::{
    AirPlayClient, ClientConfig, PreferredProtocol, SelectedProtocol, UnifiedAirPlayClient,
    check_raop_encryption,
};
pub use control::volume::Volume;
pub use discovery::{DiscoveryEvent, discover, scan};
pub use error::AirPlayError;
pub use group::{DeviceGroup, GroupId, GroupManager};
pub use player::{AirPlayPlayer, PlayerBuilder, quick_connect, quick_connect_to, quick_play};
pub use state::{ClientEvent, ClientState};
pub use types::RepeatMode;
pub use types::{AirPlayConfig, AirPlayDevice, DeviceCapabilities, PlaybackState, TrackInfo};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prelude for common imports
///
/// Convenient re-exports
pub mod prelude {
    pub use crate::AirPlayClient;
    pub use crate::AirPlayConfig;
    pub use crate::AirPlayDevice;
    pub use crate::AirPlayError;
    pub use crate::AirPlayPlayer;
    pub use crate::AudioFormat;
    pub use crate::PlaybackState;
    pub use crate::TrackInfo;
    pub use crate::Volume;

    pub use crate::discover;
    pub use crate::quick_connect;
    pub use crate::quick_connect_to;
    pub use crate::quick_play;
    pub use crate::scan;
}
