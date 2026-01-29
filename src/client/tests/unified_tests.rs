use crate::client::{ClientConfig, PreferredProtocol, SelectedProtocol, UnifiedAirPlayClient};
use crate::testing::mock_raop_server::{MockRaopConfig, MockRaopServer};
use crate::types::{AirPlayDevice, DeviceCapabilities};
use std::net::{IpAddr, Ipv4Addr};

fn create_device(airplay2: bool, raop: bool) -> AirPlayDevice {
    let mut device = AirPlayDevice {
        id: "test".to_string(),
        name: "Test Device".to_string(),
        model: None,
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
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

#[tokio::test]
async fn test_unified_client_defaults() {
    let client = UnifiedAirPlayClient::new();
    assert!(!client.is_connected());
    assert!(client.protocol().is_none());
}

#[tokio::test]
async fn test_unified_client_connect_raop() {
    // Start mock server to handle teardown request
    let mut server = MockRaopServer::new(MockRaopConfig::default());
    server.start().await.unwrap();

    let mut device = create_device(false, true);
    device.raop_port = Some(server.config.rtsp_port);

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
    let device = create_device(true, false);
    let mut client = UnifiedAirPlayClient::new();

    // AirPlay2SessionImpl::connect calls client.connect, which might fail if no real device
    // But ConnectionManager uses mDNS resolution etc.
    // So this might fail in a unit test environment if it tries to open network connections.
    // RaopSessionImpl connect was stubbed to Ok.
    // AirPlayClient connect tries to connect.

    // We should expect failure or mock it.
    // Since we can't easily mock AirPlayClient inside AirPlay2SessionImpl without refactoring injection,
    // we might skip actual connection test for AirPlay2 if it does I/O.

    // However, let's see if we can at least test selection logic.
    // If connect fails, we catch error.

    let result = client.connect(device).await;
    // It will likely fail to connect to 127.0.0.1:7000
    assert!(result.is_err());

    // But we can check if it selected AirPlay 2 before failing?
    // connect returns Result. If it fails, state is not updated (session is None).
}

#[tokio::test]
async fn test_unified_client_force_protocol() {
    let device = create_device(true, true);
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    client.connect(device).await.unwrap();
    assert_eq!(client.protocol(), Some(SelectedProtocol::Raop));
}
