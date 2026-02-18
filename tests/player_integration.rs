use airplay2::AirPlayPlayer;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use airplay2::types::AirPlayDevice;
use std::time::Duration;

#[tokio::test]
async fn test_player_integration() {
    // 1. Start Server
    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start server");

    // 2. Create Player
    let player = AirPlayPlayer::new();

    // 3. Create Device
    let device = AirPlayDevice {
        id: "player_test_dev".to_string(),
        name: "Player Test Device".to_string(),
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
    };

    // 4. Connect
    player.connect(&device).await.expect("Connect failed");
    assert!(player.is_connected().await);

    // 5. Play Tracks
    let tracks = vec![
        (
            "http://example.com/1.mp3".to_string(),
            "Track 1".to_string(),
            "Artist 1".to_string(),
        ),
        (
            "http://example.com/2.mp3".to_string(),
            "Track 2".to_string(),
            "Artist 2".to_string(),
        ),
    ];

    player
        .play_tracks(tracks)
        .await
        .expect("Play tracks failed");

    // Check playback state
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(player.is_playing().await);
    assert_eq!(player.queue_length().await, 2);

    // 6. Controls
    player.pause().await.expect("Pause failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!player.is_playing().await);

    player.play().await.expect("Resume failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(player.is_playing().await);

    player.skip().await.expect("Skip failed");
    // Verify queue length (should decrease? or index moves? PlaybackQueue implementation specific)
    // For now just ensure command succeeded

    // 7. Disconnect
    player.disconnect().await.expect("Disconnect failed");
    assert!(!player.is_connected().await);

    server.stop().await;
}
