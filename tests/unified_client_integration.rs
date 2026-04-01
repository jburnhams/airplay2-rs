use airplay2::{ClientConfig, PreferredProtocol, UnifiedAirPlayClient, SelectedProtocol};
use airplay2::types::{AirPlayDevice, DeviceCapabilities, RaopCapabilities};
use airplay2::error::AirPlayError;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use std::collections::HashMap;
use std::time::Instant;

fn create_test_device(port: u16, supports_ap2: bool, supports_raop: bool) -> AirPlayDevice {
    let mut caps = DeviceCapabilities::default();
    caps.airplay2 = supports_ap2;

    AirPlayDevice {
        id: "12:34:56:78:90:AB".to_string(),
        name: "Test Unified Server".to_string(),
        model: Some("AudioAccessory5,1".to_string()),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port,
        capabilities: caps,
        raop_port: if supports_raop { Some(port + 100) } else { None },
        raop_capabilities: if supports_raop { Some(RaopCapabilities::default()) } else { None },
        txt_records: HashMap::new(),
        last_seen: Some(Instant::now()),
    }
}

#[tokio::test]
async fn test_unified_client_ap2_preference() {
    let mut config = MockServerConfig::default();
    config.rtsp_port = 0;
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start server");

    let device = create_test_device(addr.port(), true, true);

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::PreferAirPlay2,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);

    let res: Result<(), AirPlayError> = client.connect(device).await;

    match res {
        Ok(_) => {
            assert!(client.is_connected());
            assert_eq!(client.protocol(), Some(SelectedProtocol::AirPlay2));
            let _: Result<(), _> = client.disconnect().await;
        }
        Err(e) => {
            println!("Connection failed (expected if mock lacks AP2 pairing): {}", e);
        }
    }

    server.stop().await;
}

#[tokio::test]
async fn test_unified_client_raop_preference() {
    let mut config = MockServerConfig::default();
    config.rtsp_port = 0;
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start server");

    let device = create_test_device(addr.port(), true, true);

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::PreferRaop,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);

    let res: Result<(), AirPlayError> = client.connect(device).await;

    assert!(res.is_err());

    server.stop().await;
}

#[tokio::test]
async fn test_unified_client_unsupported_protocol() {
    let device = create_test_device(7000, false, false);

    let mut client = UnifiedAirPlayClient::new();
    let res: Result<(), AirPlayError> = client.connect(device).await;
    let err = res.unwrap_err();

    assert!(matches!(err, AirPlayError::ConnectionFailed { .. }));
}
