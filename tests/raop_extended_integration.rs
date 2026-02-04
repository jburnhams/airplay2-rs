//! Extended RAOP integration tests

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
async fn test_raop_reconnection() {
    let server = setup_mock_server().await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);
    let device = create_device(&server, "reconnect-device").await;

    // First connection
    client
        .connect(device.clone())
        .await
        .expect("connect 1 failed");
    assert!(client.is_connected());

    // Disconnect
    client.disconnect().await.expect("disconnect 1 failed");
    assert!(!client.is_connected());

    // Second connection
    client.connect(device).await.expect("connect 2 failed");
    assert!(client.is_connected());

    client.disconnect().await.expect("disconnect 2 failed");
}

#[tokio::test]
async fn test_raop_metadata_and_artwork_updates() {
    let server = setup_mock_server().await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        enable_metadata: true,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);
    let device = create_device(&server, "meta-device").await;

    client.connect(device).await.expect("connect failed");

    // Set Metadata
    let track = TrackInfo {
        title: "Extended Test".to_string(),
        artist: "QA Agent".to_string(),
        ..Default::default()
    };

    if let Some(session) = client.session_mut() {
        session.set_metadata(&track).await.expect("metadata failed");
    }

    // Set Artwork
    let artwork = vec![0x1, 0x2, 0x3, 0x4];
    if let Some(session) = client.session_mut() {
        session.set_artwork(&artwork).await.expect("artwork failed");
    }

    // Wait for server to receive
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify
    let state = server.state();
    assert!(state.metadata.is_some(), "Metadata should be received");
    assert!(state.artwork.is_some(), "Artwork should be received");
    assert_eq!(state.artwork.unwrap(), artwork);

    client.disconnect().await.ok();
}

#[tokio::test]
async fn test_raop_volume_control() {
    let server = setup_mock_server().await;
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);
    let device = create_device(&server, "vol-device").await;

    client.connect(device).await.expect("connect failed");

    // Set Volume
    // 0.75 should result in non-zero dB (likely negative)
    client.set_volume(0.75).await.expect("set volume failed");

    // Wait for server
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify
    let state = server.state();

    // Default volume_db is 0.0 in MockRaopState.
    // If the client sends anything, it should change.
    // Note: If 0.75 maps to 0.0dB (max), then this assertion fails.
    // But usually 1.0 is max (0dB). 0.75 is less.
    // Also, usually default might be initialized to something else?
    // MockRaopState::default() is derived, so 0.0.

    // Let's send a very low volume to ensure it's distinct from 0.0 (if 0.0 is silent?)
    // Actually, in dB, 0.0 is usually MAX. -30.0 is silent.
    // So if default is 0.0 (Max), and we send 0.75 (less than max), we expect < 0.0.

    assert!(
        state.volume_db != 0.0,
        "Volume should be updated (expected != 0.0, got {})",
        state.volume_db
    );

    client.disconnect().await.ok();
}
