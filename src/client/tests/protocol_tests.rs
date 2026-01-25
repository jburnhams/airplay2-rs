use crate::client::protocol::{
    PreferredProtocol, ProtocolError, SelectedProtocol, select_protocol,
};
use crate::types::{AirPlayDevice, DeviceCapabilities};

fn create_device(airplay2: bool, raop: bool) -> AirPlayDevice {
    let mut device = AirPlayDevice {
        id: "test".to_string(),
        name: "Test Device".to_string(),
        model: None,
        addresses: vec![],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    if airplay2 {
        device.capabilities.airplay2 = true;
    }
    if raop {
        device.raop_port = Some(5000);
    }

    device
}

#[test]
fn test_select_protocol_force_airplay2() {
    let device_ap2 = create_device(true, false);
    let device_raop = create_device(false, true);
    let device_both = create_device(true, true);

    assert_eq!(
        select_protocol(&device_ap2, PreferredProtocol::ForceAirPlay2).unwrap(),
        SelectedProtocol::AirPlay2
    );
    assert!(matches!(
        select_protocol(&device_raop, PreferredProtocol::ForceAirPlay2),
        Err(ProtocolError::AirPlay2NotSupported)
    ));
    assert_eq!(
        select_protocol(&device_both, PreferredProtocol::ForceAirPlay2).unwrap(),
        SelectedProtocol::AirPlay2
    );
}

#[test]
fn test_select_protocol_force_raop() {
    let device_ap2 = create_device(true, false);
    let device_raop = create_device(false, true);
    let device_both = create_device(true, true);

    assert!(matches!(
        select_protocol(&device_ap2, PreferredProtocol::ForceRaop),
        Err(ProtocolError::RaopNotSupported)
    ));
    assert_eq!(
        select_protocol(&device_raop, PreferredProtocol::ForceRaop).unwrap(),
        SelectedProtocol::Raop
    );
    assert_eq!(
        select_protocol(&device_both, PreferredProtocol::ForceRaop).unwrap(),
        SelectedProtocol::Raop
    );
}

#[test]
fn test_select_protocol_prefer_airplay2() {
    let device_ap2 = create_device(true, false);
    let device_raop = create_device(false, true);
    let device_both = create_device(true, true);

    assert_eq!(
        select_protocol(&device_ap2, PreferredProtocol::PreferAirPlay2).unwrap(),
        SelectedProtocol::AirPlay2
    );
    assert_eq!(
        select_protocol(&device_raop, PreferredProtocol::PreferAirPlay2).unwrap(),
        SelectedProtocol::Raop
    );
    assert_eq!(
        select_protocol(&device_both, PreferredProtocol::PreferAirPlay2).unwrap(),
        SelectedProtocol::AirPlay2
    );
}

#[test]
fn test_select_protocol_prefer_raop() {
    let device_ap2 = create_device(true, false);
    let device_raop = create_device(false, true);
    let device_both = create_device(true, true);

    assert_eq!(
        select_protocol(&device_ap2, PreferredProtocol::PreferRaop).unwrap(),
        SelectedProtocol::AirPlay2
    );
    assert_eq!(
        select_protocol(&device_raop, PreferredProtocol::PreferRaop).unwrap(),
        SelectedProtocol::Raop
    );
    assert_eq!(
        select_protocol(&device_both, PreferredProtocol::PreferRaop).unwrap(),
        SelectedProtocol::Raop
    );
}

#[test]
fn test_select_protocol_none_supported() {
    let device = create_device(false, false);
    assert!(matches!(
        select_protocol(&device, PreferredProtocol::PreferAirPlay2),
        Err(ProtocolError::NoSupportedProtocol)
    ));
}
