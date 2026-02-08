use airplay2::testing::mock_raop_server::{MockRaopConfig, MockRaopServer};
use airplay2::types::{DeviceCapabilities, RaopCapabilities, TrackInfo};
use airplay2::{AirPlayDevice, ClientConfig, PreferredProtocol, UnifiedAirPlayClient};
use std::collections::HashMap;
use std::time::Duration;

async fn setup_mock_server() -> MockRaopServer {
    let config = MockRaopConfig {
        rtsp_port: 0,
        audio_port: 0,
        ..Default::default()
    };
    let mut server = MockRaopServer::new(config);
    server.start().await.expect("failed to start mock server");
    server
}

async fn create_device(server: &MockRaopServer, id: &str) -> AirPlayDevice {
    AirPlayDevice {
        id: id.to_string(),
        name: server.service_name(),
        model: Some("TestModel".to_string()),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 0,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(server.config.rtsp_port),
        raop_capabilities: Some(RaopCapabilities::default()),
        txt_records: HashMap::new(),
    }
}

#[tokio::test]
async fn test_connect_disconnect_loop() {
    let server = setup_mock_server().await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    // Perform 20 cycles
    for i in 0..20 {
        let mut client = UnifiedAirPlayClient::with_config(config.clone());
        let device = create_device(&server, &format!("cycle-device-{}", i)).await;

        client.connect(device).await.expect("connect failed");
        assert!(client.is_connected());

        client.disconnect().await.expect("disconnect failed");
        assert!(!client.is_connected());
    }
}

#[tokio::test]
async fn test_metadata_flood() {
    let server = setup_mock_server().await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        enable_metadata: true,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);
    let device = create_device(&server, "flood-device").await;

    client.connect(device).await.expect("connect failed");

    // Flood 100 metadata updates
    for i in 0..100 {
        let track = TrackInfo {
            title: format!("Title {}", i),
            artist: "Artist".to_string(),
            album: Some("Album".to_string()),
            ..Default::default()
        };

        if let Some(session) = client.session_mut() {
            session.set_metadata(&track).await.expect("metadata failed");
        }
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify last state
    let state = server.state();
    // Assuming mock server stores last metadata
    assert!(state.metadata.is_some());

    client.disconnect().await.expect("clean disconnect failed");
}
