use airplay2::AirPlayPlayer;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use airplay2::types::AirPlayDevice;
use std::time::Duration;

#[tokio::test]
async fn test_rate_control_integration() {
    common::init_logging();
    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start server");

    let player = AirPlayPlayer::new();
    let device = AirPlayDevice {
        id: "rate_test_dev".to_string(),
        name: "Rate Test Device".to_string(),
        model: Some("Mock".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: airplay2::types::DeviceCapabilities {
            airplay2: true,
            supports_audio: true,
            ..Default::default()
        },
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
        last_seen: None,
    };

    player.connect(&device).await.expect("Connect failed");
    assert!(player.is_connected().await);

    player
        .play_track(
            "http://example.com/single.mp3",
            "Single Track",
            "Solo Artist",
        )
        .await
        .expect("Play track failed");

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Test fast_forward via player facade
    player.fast_forward().await.expect("Fast forward failed");

    // Test rewind via player facade
    player.rewind().await.expect("Rewind failed");

    player.disconnect().await.expect("Disconnect failed");
    server.stop().await;
}

mod common;
