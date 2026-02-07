use crate::client::{ClientConfig, PreferredProtocol, SelectedProtocol, UnifiedAirPlayClient};
use crate::testing::mock_raop_server::{MockRaopConfig, MockRaopServer};
use crate::types::{AirPlayDevice, DeviceCapabilities};
use std::net::{IpAddr, Ipv4Addr};

async fn create_device_with_server(
    airplay2: bool,
    raop: bool,
) -> (AirPlayDevice, Option<MockRaopServer>) {
    let mut server = None;
    let mut raop_port = None;

    if raop {
        let config = MockRaopConfig {
            rtsp_port: 0, // Dynamic
            ..Default::default()
        };
        let mut s = MockRaopServer::new(config);
        s.start().await.expect("failed to start mock server");
        raop_port = Some(s.config.rtsp_port);
        server = Some(s);
    }

    let mut device = AirPlayDevice {
        id: "test".to_string(),
        name: "Test Device".to_string(),
        model: None,
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    if airplay2 {
        device.capabilities.airplay2 = true;
    }

    (device, server)
}

#[tokio::test]
async fn test_unified_client_defaults() {
    let client = UnifiedAirPlayClient::new();
    assert!(!client.is_connected());
    assert!(client.protocol().is_none());
}

#[tokio::test]
async fn test_unified_client_connect_raop() {
    let (device, _server) = create_device_with_server(false, true).await;
    let mut client = UnifiedAirPlayClient::new();

    client.connect(device).await.unwrap();

    assert!(client.is_connected());
    assert_eq!(client.protocol(), Some(SelectedProtocol::Raop));

    // Check session type indirectly by checking protocol version or behavior
    let session = client.session().unwrap();
    assert_eq!(session.protocol_version(), "RAOP/1.0");

    client.disconnect().await.unwrap();
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_unified_client_connect_airplay2() {
    let (device, _server) = create_device_with_server(true, false).await;
    let mut client = UnifiedAirPlayClient::new();

    // AirPlay2 connection will likely fail as we don't mock it here,
    // but we expect it to TRY connecting.
    let result = client.connect(device).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unified_client_force_protocol() {
    let (device, _server) = create_device_with_server(true, true).await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    client.connect(device).await.unwrap();
    assert_eq!(client.protocol(), Some(SelectedProtocol::Raop));
}

#[tokio::test]
async fn test_connection_failure_handling() {
    let device = AirPlayDevice {
        id: "test".to_string(),
        name: "Test Device".to_string(),
        model: None,
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(12345), // Random port likely closed
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    // Configure to force RAOP to use that port
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);
    let result = client.connect(device).await;

    assert!(result.is_err());
    assert!(!client.is_connected());
    assert!(client.protocol().is_none());
}

#[tokio::test]
async fn test_protocol_preference_e2e() {
    // Device with both, but only RAOP server running
    let (device, _server) = create_device_with_server(true, true).await;

    // 1. Prefer AirPlay 2 (default) -> Should try AP2 and fail
    let mut client = UnifiedAirPlayClient::new();
    let result = client.connect(device.clone()).await;
    assert!(result.is_err()); // Failed to connect to AP2

    // 2. Prefer RAOP -> Should connect to RAOP
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::PreferRaop,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);
    client.connect(device).await.unwrap();
    assert_eq!(client.protocol(), Some(SelectedProtocol::Raop));
}
