use std::time::Duration;

use airplay2::AirPlayClient;
use airplay2::protocol::rtsp::Method;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use airplay2::types::{AirPlayConfig, AirPlayDevice, TimingProtocol};
use tokio::time::timeout;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
async fn test_client_ptp_handshake() {
    init_tracing();

    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    let client_config = AirPlayConfig {
        timing_protocol: TimingProtocol::Ptp,
        ..Default::default()
    };
    let client = AirPlayClient::new(client_config);

    let device = AirPlayDevice {
        id: "mock_ptp_device".to_string(),
        name: "Mock PTP Device".to_string(),
        model: Some("MockModel".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: airplay2::types::DeviceCapabilities {
            airplay2: true,
            supports_ptp: true,
            supports_audio: true,
            supports_buffered_audio: true,
            ..Default::default()
        },
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    // Connect should trigger PTP handshake including SetPeers
    timeout(Duration::from_secs(5), client.connect(&device))
        .await
        .expect("Connection timed out")
        .expect("Connection failed");

    assert!(client.is_connected().await);

    // Verify SetPeers was received
    let methods = server.received_methods().await;
    assert!(
        methods.contains(&Method::SetPeers),
        "Server should have received SETPEERS"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_client_playback_control_methods() {
    init_tracing();

    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    let client = AirPlayClient::default_client();
    let device = AirPlayDevice {
        id: "mock_device_playback".to_string(),
        name: "Mock Device Playback".to_string(),
        model: Some("MockModel".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: Default::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    client.connect(&device).await.expect("Connect failed");

    // Play -> SetRateAnchorTime
    client.play().await.expect("Play failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.is_streaming().await);

    let methods = server.received_methods().await;
    assert!(
        methods.contains(&Method::SetRateAnchorTime),
        "Server should have received SETRATEANCHORTIME"
    );

    // Pause -> SetRateAnchorTime (rate 0)
    client.pause().await.expect("Pause failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!server.is_streaming().await);

    server.stop().await;
}
