use std::time::Duration;

use airplay2::error::AirPlayError;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use airplay2::types::AirPlayDevice;
use airplay2::{AirPlayPlayer, PlayerBuilder};

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
        last_seen: None,
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

    let mut rx = player.client().subscribe_state();

    player
        .play_tracks(tracks)
        .await
        .expect("Play tracks failed");

    // Check playback state
    // Wait for state to become playing
    tokio::time::timeout(Duration::from_secs(2), async {
        while !rx.borrow_and_update().playback.is_playing {
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("Timeout waiting for playback to start");

    assert!(player.is_playing().await);
    assert_eq!(player.queue_length().await, 2);

    // 6. Controls
    player.pause().await.expect("Pause failed");

    // Wait for state to become paused
    tokio::time::timeout(Duration::from_secs(2), async {
        while rx.borrow_and_update().playback.is_playing {
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("Timeout waiting for playback to pause");

    assert!(!player.is_playing().await);

    player.play().await.expect("Resume failed");

    // Wait for state to become playing
    tokio::time::timeout(Duration::from_secs(2), async {
        while !rx.borrow_and_update().playback.is_playing {
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("Timeout waiting for playback to resume");

    assert!(player.is_playing().await);

    player.skip().await.expect("Skip failed");
    // Verify queue length (should decrease? or index moves? PlaybackQueue implementation specific)
    // For now just ensure command succeeded

    // 7. Disconnect
    player.disconnect().await.expect("Disconnect failed");
    assert!(!player.is_connected().await);

    server.stop().await;
}

#[tokio::test]
async fn test_player_advanced_controls() {
    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start server");

    let player = AirPlayPlayer::new();
    let device = AirPlayDevice {
        id: "player_test_advanced".to_string(),
        name: "Advanced Test Device".to_string(),
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

    player
        .play_track(
            "http://example.com/single.mp3",
            "Single Track",
            "Solo Artist",
        )
        .await
        .expect("Play track failed");

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(player.is_playing().await);
    assert_eq!(player.queue_length().await, 1);

    player.set_volume(0.8).await.expect("Set volume failed");
    let vol = player.volume().await;
    assert!((vol - 0.8).abs() < f32::EPSILON);

    player.mute().await.expect("Mute failed");
    assert!(
        player.client().state().await.muted,
        "Player should be in a muted state"
    );

    player.unmute().await.expect("Unmute failed");
    assert!(
        !player.client().state().await.muted,
        "Player should be in an unmuted state"
    );

    player.repeat_one().await.expect("Repeat one failed");
    player.repeat_all().await.expect("Repeat all failed");
    player.repeat_off().await.expect("Repeat off failed");

    player.shuffle_on().await.expect("Shuffle on failed");
    player.shuffle_off().await.expect("Shuffle off failed");

    player.seek(15.0).await.expect("Seek failed");

    // Test fast forward
    player.fast_forward().await.expect("Fast forward failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let rate = server.rate().await;
    assert!(
        (rate - 2.0).abs() < f64::EPSILON,
        "Expected rate 2.0, got {}",
        rate
    );

    // Test rewind
    player.rewind().await.expect("Rewind failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let rate = server.rate().await;
    assert!(
        (rate + 2.0).abs() < f64::EPSILON,
        "Expected rate -2.0, got {}",
        rate
    );

    player.disconnect().await.expect("Disconnect failed");
    server.stop().await;
}

#[tokio::test]
async fn test_player_builder() {
    let player = PlayerBuilder::new()
        .connection_timeout(Duration::from_secs(5))
        .auto_reconnect(false)
        .device_name("TestDevice")
        .build();

    assert!(!player.is_connected().await);
    assert_eq!(player.queue_length().await, 0);
}

#[tokio::test]
async fn test_player_disconnected_errors() {
    let player = AirPlayPlayer::new();

    // Verify operations fail gracefully when disconnected
    let res = player.play().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.pause().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.set_volume(0.5).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.stop().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.skip().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.seek(10.0).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}
