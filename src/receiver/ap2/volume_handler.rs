//! Volume Control for AirPlay 2 Receiver

use std::sync::{Arc, RwLock};

use super::body_handler::parse_text_parameters;
use super::request_handler::{Ap2Event, Ap2HandleResult};
use super::response_builder::Ap2ResponseBuilder;
use crate::protocol::rtsp::{RtspRequest, StatusCode};

/// Volume controller
pub struct VolumeController {
    /// Current volume in dB (-144 to 0)
    volume_db: Arc<RwLock<f32>>,
    /// Muted flag
    muted: Arc<RwLock<bool>>,
}

impl VolumeController {
    /// Create a new volume controller with default volume
    #[must_use]
    pub fn new() -> Self {
        Self {
            volume_db: Arc::new(RwLock::new(-20.0)), // Default -20dB
            muted: Arc::new(RwLock::new(false)),
        }
    }

    /// Set volume in dB (clamped to -144..0)
    pub fn set_volume_db(&self, volume: f32) {
        let clamped = volume.clamp(-144.0, 0.0);
        if let Ok(mut v) = self.volume_db.write() {
            *v = clamped;
        }
    }

    /// Get current volume in dB
    #[must_use]
    pub fn volume_db(&self) -> f32 {
        *self.volume_db.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Convert dB to linear (0.0 to 1.0)
    #[must_use]
    pub fn volume_linear(&self) -> f32 {
        let db = self.volume_db();
        if db <= -144.0 {
            0.0
        } else {
            10.0_f32.powf(db / 20.0)
        }
    }

    /// Set muted state
    pub fn set_muted(&self, muted: bool) {
        if let Ok(mut m) = self.muted.write() {
            *m = muted;
        }
    }

    /// Check if muted
    #[must_use]
    pub fn is_muted(&self) -> bool {
        *self.muted.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Handle SET_PARAMETER with volume
    ///
    /// # Errors
    ///
    /// Returns `VolumeError` if the body format is invalid or volume is missing.
    pub fn handle_set_volume(&self, body: &[u8]) -> Result<f32, VolumeError> {
        let params = parse_text_parameters(body).map_err(|_| VolumeError::ParseError)?;

        let volume_str = params.get("volume").ok_or(VolumeError::MissingVolume)?;

        let volume: f32 = volume_str.parse().map_err(|_| VolumeError::InvalidValue)?;

        self.set_volume_db(volume);
        Ok(volume)
    }
}

impl Default for VolumeController {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors for volume handling
#[derive(Debug, thiserror::Error)]
pub enum VolumeError {
    /// Failed to parse volume parameters
    #[error("Failed to parse volume parameters")]
    ParseError,
    /// Missing volume parameter
    #[error("Missing volume parameter")]
    MissingVolume,
    /// Invalid volume value
    #[error("Invalid volume value")]
    InvalidValue,
}

/// Handle SET_PARAMETER for volume
///
/// # Arguments
///
/// * `request` - The RTSP request
/// * `cseq` - Sequence number
/// * `controller` - Volume controller instance
#[must_use]
pub fn handle_volume_set_parameter(
    request: &RtspRequest,
    cseq: u32,
    controller: &VolumeController,
) -> Ap2HandleResult {
    match controller.handle_set_volume(&request.body) {
        Ok(volume) => Ap2HandleResult {
            response: Ap2ResponseBuilder::ok().cseq(cseq).encode(),
            new_state: None,
            event: Some(Ap2Event::VolumeChanged { volume }),
            error: None,
        },
        Err(e) => Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_clamping() {
        let vc = VolumeController::new();

        vc.set_volume_db(-200.0);
        assert_eq!(vc.volume_db(), -144.0);

        vc.set_volume_db(10.0);
        assert_eq!(vc.volume_db(), 0.0);
    }

    #[test]
    fn test_volume_linear() {
        let vc = VolumeController::new();

        vc.set_volume_db(0.0);
        assert!((vc.volume_linear() - 1.0).abs() < f32::EPSILON);

        vc.set_volume_db(-20.0);
        assert!((vc.volume_linear() - 0.1).abs() < f32::EPSILON);

        vc.set_volume_db(-144.0);
        assert_eq!(vc.volume_linear(), 0.0);
    }

    #[test]
    fn test_handle_set_volume() {
        let vc = VolumeController::new();
        let body = b"volume: -15.5\r\n";

        let vol = vc.handle_set_volume(body).unwrap();
        assert_eq!(vol, -15.5);
        assert_eq!(vc.volume_db(), -15.5);
    }
}
