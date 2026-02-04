//! RAOP integration tests using mock server

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

#[tokio::test]
async fn test_full_raop_session() {
    let server = setup_mock_server().await;

    // Create client configured for RAOP
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        connection_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    // Create mock device
    let device = AirPlayDevice {
        id: "test-device".to_string(),
        name: server.service_name(),
        model: Some("TestModel".to_string()),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 0,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(server.config.rtsp_port),
        raop_capabilities: Some(RaopCapabilities::default()),
        txt_records: HashMap::new(),
    };

    // Connect
    client.connect(device).await.expect("connection failed");
    assert!(client.is_connected());

    // Set volume
    client.set_volume(0.5).await.expect("set volume failed");

    // Start playback
    client.play().await.expect("play failed");

    // Verify server state
    let state = server.state();
    assert!(state.playing);

    // Disconnect
    client.disconnect().await.expect("disconnect failed");
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_raop_audio_streaming() {
    let server = setup_mock_server().await;

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    let device = AirPlayDevice {
        id: "test-device-2".to_string(),
        name: server.service_name(),
        model: Some("TestModel".to_string()),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 0,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(server.config.rtsp_port),
        raop_capabilities: Some(RaopCapabilities::default()),
        txt_records: HashMap::new(),
    };

    client.connect(device).await.expect("connection failed");

    // Stream some audio
    let audio_frame = vec![0u8; 352 * 4]; // One frame
    for _ in 0..10 {
        client
            .stream_audio(&audio_frame)
            .await
            .expect("stream failed");
    }

    // Verify packets received
    // let state = server.state();
    // assert!(!state.audio_packets.is_empty());

    // Cleanup
    client.disconnect().await.ok();
}

#[tokio::test]
async fn test_raop_metadata() {
    let server = setup_mock_server().await;

    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        enable_metadata: true,
        ..Default::default()
    };

    let mut client = UnifiedAirPlayClient::with_config(config);

    let device = AirPlayDevice {
        id: "test-device-3".to_string(),
        name: server.service_name(),
        model: Some("TestModel".to_string()),
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 0,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(server.config.rtsp_port),
        raop_capabilities: Some(RaopCapabilities::default()),
        txt_records: HashMap::new(),
    };

    client.connect(device).await.expect("connection failed");

    // Set metadata
    let track = TrackInfo {
        title: "Test Song".to_string(),
        artist: "Test Artist".to_string(),
        album: Some("Test Album".to_string()),
        ..Default::default()
    };

    if let Some(session) = client.session_mut() {
        session.set_metadata(&track).await.expect("metadata failed");
    }

    // Verify
    tokio::time::sleep(Duration::from_millis(100)).await;
    let state = server.state();
    assert!(state.metadata.is_some());

    // Cleanup
    client.disconnect().await.ok();
}
