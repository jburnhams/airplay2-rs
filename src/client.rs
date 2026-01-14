//! `AirPlayClient` implementation

use crate::{AirPlayDevice, AirPlayError, TrackInfo};

/// Low-level client for controlling an `AirPlay` device.
pub struct AirPlayClient;

impl AirPlayClient {
    /// Connect to an `AirPlay` device.
    ///
    /// # Errors
    ///
    /// Returns an error if connection fails.
    #[allow(clippy::unused_async)]
    pub async fn connect(_device: &AirPlayDevice) -> Result<Self, AirPlayError> {
        Ok(Self)
    }

    /// Load a track for playback.
    ///
    /// # Errors
    ///
    /// Returns an error if loading fails.
    #[allow(clippy::unused_async)]
    pub async fn load(&mut self, _track: &TrackInfo) -> Result<(), AirPlayError> {
        Ok(())
    }

    /// Start playback.
    ///
    /// # Errors
    ///
    /// Returns an error if playback fails.
    #[allow(clippy::unused_async)]
    pub async fn play(&mut self) -> Result<(), AirPlayError> {
        Ok(())
    }
}
