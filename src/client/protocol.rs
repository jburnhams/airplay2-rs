//! Protocol detection and selection

use crate::types::{AirPlayDevice, RaopCapabilities};

/// Preferred protocol for connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferredProtocol {
    /// Prefer `AirPlay` 2 when available
    #[default]
    PreferAirPlay2,
    /// Prefer `AirPlay` 1 (RAOP) when available
    PreferRaop,
    /// Force `AirPlay` 2 only
    ForceAirPlay2,
    /// Force RAOP only
    ForceRaop,
}

/// Protocol selection result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedProtocol {
    /// Use `AirPlay` 2
    AirPlay2,
    /// Use `AirPlay` 1 (RAOP)
    Raop,
}

/// Protocol selection error
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// `AirPlay` 2 not supported by device
    #[error("AirPlay 2 not supported by device")]
    AirPlay2NotSupported,
    /// RAOP not supported by device
    #[error("RAOP not supported by device")]
    RaopNotSupported,
    /// No supported protocol available
    #[error("no supported protocol available")]
    NoSupportedProtocol,
    /// Unsupported encryption type
    #[error("unsupported encryption type")]
    UnsupportedEncryption,
}

/// Select protocol for device connection
///
/// # Errors
///
/// Returns `ProtocolError` if the preferred protocol cannot be satisfied.
pub fn select_protocol(
    device: &AirPlayDevice,
    preferred: PreferredProtocol,
) -> Result<SelectedProtocol, ProtocolError> {
    match preferred {
        PreferredProtocol::ForceAirPlay2 => {
            if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else {
                Err(ProtocolError::AirPlay2NotSupported)
            }
        }
        PreferredProtocol::ForceRaop => {
            if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else {
                Err(ProtocolError::RaopNotSupported)
            }
        }
        PreferredProtocol::PreferAirPlay2 => {
            if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else {
                Err(ProtocolError::NoSupportedProtocol)
            }
        }
        PreferredProtocol::PreferRaop => {
            if device.supports_raop() {
                Ok(SelectedProtocol::Raop)
            } else if device.supports_airplay2() {
                Ok(SelectedProtocol::AirPlay2)
            } else {
                Err(ProtocolError::NoSupportedProtocol)
            }
        }
    }
}

/// Check if RAOP encryption is compatible
///
/// # Errors
///
/// Returns `ProtocolError::UnsupportedEncryption` if no supported encryption type is found.
pub fn check_raop_encryption(caps: &RaopCapabilities) -> Result<(), ProtocolError> {
    if let Some(enc) = caps.preferred_encryption() {
        if enc.is_supported() {
            Ok(())
        } else {
            Err(ProtocolError::UnsupportedEncryption)
        }
    } else {
        Err(ProtocolError::UnsupportedEncryption)
    }
}
