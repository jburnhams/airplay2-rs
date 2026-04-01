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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AirPlayDevice, DeviceCapabilities, RaopCapabilities, RaopEncryption};

    fn create_device(supports_ap2: bool, supports_raop: bool) -> AirPlayDevice {
        let caps = DeviceCapabilities {
            airplay2: supports_ap2,
            ..Default::default()
        };

        AirPlayDevice {
            id: "12:34:56:78:90:AB".to_string(),
            name: "Test Device".to_string(),
            model: Some("TestModel".to_string()),
            port: 7000,
            capabilities: caps,
            raop_port: if supports_raop { Some(5000) } else { None },
            raop_capabilities: Some(RaopCapabilities::default()),
            txt_records: std::collections::HashMap::new(),
            last_seen: Some(std::time::Instant::now()),
            addresses: vec![],
        }
    }

    #[test]
    fn test_select_protocol_force_ap2() {
        let device = create_device(true, false);
        assert_eq!(
            select_protocol(&device, PreferredProtocol::ForceAirPlay2).unwrap(),
            SelectedProtocol::AirPlay2
        );

        let device_unsupported = create_device(false, true);
        assert!(matches!(
            select_protocol(&device_unsupported, PreferredProtocol::ForceAirPlay2),
            Err(ProtocolError::AirPlay2NotSupported)
        ));
    }

    #[test]
    fn test_select_protocol_force_raop() {
        let device = create_device(false, true);
        assert_eq!(
            select_protocol(&device, PreferredProtocol::ForceRaop).unwrap(),
            SelectedProtocol::Raop
        );

        let device_unsupported = create_device(true, false);
        assert!(matches!(
            select_protocol(&device_unsupported, PreferredProtocol::ForceRaop),
            Err(ProtocolError::RaopNotSupported)
        ));
    }

    #[test]
    fn test_select_protocol_prefer_ap2() {
        let device_both = create_device(true, true);
        assert_eq!(
            select_protocol(&device_both, PreferredProtocol::PreferAirPlay2).unwrap(),
            SelectedProtocol::AirPlay2
        );

        let device_raop_only = create_device(false, true);
        assert_eq!(
            select_protocol(&device_raop_only, PreferredProtocol::PreferAirPlay2).unwrap(),
            SelectedProtocol::Raop
        );

        let device_none = create_device(false, false);
        assert!(matches!(
            select_protocol(&device_none, PreferredProtocol::PreferAirPlay2),
            Err(ProtocolError::NoSupportedProtocol)
        ));
    }

    #[test]
    fn test_select_protocol_prefer_raop() {
        let device_both = create_device(true, true);
        assert_eq!(
            select_protocol(&device_both, PreferredProtocol::PreferRaop).unwrap(),
            SelectedProtocol::Raop
        );

        let device_ap2_only = create_device(true, false);
        assert_eq!(
            select_protocol(&device_ap2_only, PreferredProtocol::PreferRaop).unwrap(),
            SelectedProtocol::AirPlay2
        );

        let device_none = create_device(false, false);
        assert!(matches!(
            select_protocol(&device_none, PreferredProtocol::PreferRaop),
            Err(ProtocolError::NoSupportedProtocol)
        ));
    }

    #[test]
    fn test_check_raop_encryption() {
        let mut caps = RaopCapabilities {
            encryption_types: vec![],
            ..Default::default()
        };
        // Empty encryption types -> Unsupported
        assert!(matches!(
            check_raop_encryption(&caps),
            Err(ProtocolError::UnsupportedEncryption)
        ));

        caps.encryption_types = vec![RaopEncryption::Rsa];
        assert!(check_raop_encryption(&caps).is_ok());

        caps.encryption_types = vec![RaopEncryption::None];
        assert!(check_raop_encryption(&caps).is_ok());

        caps.encryption_types = vec![RaopEncryption::FairPlay];
        assert!(matches!(
            check_raop_encryption(&caps),
            Err(ProtocolError::UnsupportedEncryption)
        ));
    }
}
