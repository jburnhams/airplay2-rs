use airplay2::error::AirPlayError;
use airplay2::types::{AirPlayDevice, DeviceCapabilities};
use airplay2::{ClientConfig, PreferredProtocol, UnifiedAirPlayClient};

#[tokio::test]
async fn test_unified_client_force_raop_error_propagation() {
    let mut client = UnifiedAirPlayClient::with_config(ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    });

    let device = AirPlayDevice {
        id: "mock_device_no_raop".to_string(),
        name: "Mock Device".to_string(),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: None, // No RAOP port
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
        last_seen: None,
    };

    let result: Result<(), AirPlayError> = client.connect(device).await;
    assert!(
        result.is_err(),
        "Should fail if RAOP is forced but not supported"
    );
}

#[tokio::test]
async fn test_unified_client_force_ap2_error_propagation() {
    let mut client = UnifiedAirPlayClient::with_config(ClientConfig {
        preferred_protocol: PreferredProtocol::ForceAirPlay2,
        ..Default::default()
    });

    let device = AirPlayDevice {
        id: "mock_device_no_ap2".to_string(),
        name: "Mock Device".to_string(),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 7000,
        capabilities: DeviceCapabilities {
            airplay2: false, // No AirPlay2
            ..Default::default()
        },
        raop_port: Some(5000),
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
        last_seen: None,
    };

    let result: Result<(), AirPlayError> = client.connect(device).await;
    assert!(
        result.is_err(),
        "Should fail if AP2 is forced but not supported"
    );
}

#[tokio::test]
async fn test_unified_client_disconnected_playback_controls() {
    let mut client = UnifiedAirPlayClient::new();

    // Playback operations without connection
    let res = client.play().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.pause().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.stop().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.set_volume(0.5).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.stream_audio(&[0u8; 100]).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[tokio::test]
async fn test_unified_client_session_state() {
    let mut client = UnifiedAirPlayClient::new();
    assert!(client.session().is_none());
    assert!(client.session_mut().is_none());
    assert!(!client.is_connected());
    assert!(client.protocol().is_none());
}

#[tokio::test]
async fn test_unified_client_disconnect_when_not_connected() {
    let mut client = UnifiedAirPlayClient::new();
    // Disconnecting when not connected should not panic, but return Ok(())
    let result: Result<(), AirPlayError> = client.disconnect().await;
    assert!(result.is_ok());
}
