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
//!     let client = AirPlayClient::connect(device).await?;
//!
//!     // Stream audio...
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Public modules
pub mod error;
pub mod types;

// Internal modules
mod audio;
mod client;
mod connection;
mod control;
mod discovery;
mod group;
mod net;
mod player;
mod protocol;

// Re-exports
pub use client::AirPlayClient;
pub use error::AirPlayError;
pub use group::AirPlayGroup;
pub use player::AirPlayPlayer;
pub use types::{AirPlayConfig, AirPlayDevice, PlaybackInfo, PlaybackState, RepeatMode, TrackInfo};

// Discovery functions
pub use discovery::{discover, scan};
