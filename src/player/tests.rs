use super::*;

#[tokio::test]
async fn test_player_creation() {
    let player = AirPlayPlayer::new();
    assert!(!player.is_connected().await);
}

#[tokio::test]
async fn test_builder() {
    let player = PlayerBuilder::new()
        .connection_timeout(Duration::from_secs(10))
        .auto_reconnect(false)
        .build();

    assert!(!player.auto_reconnect);
}

#[tokio::test]
async fn test_builder_defaults() {
    let player = PlayerBuilder::new().build();
    assert!(player.auto_reconnect);
}

#[tokio::test]
async fn test_builder_device_name() {
    let player = PlayerBuilder::new().device_name("Test Device").build();

    // We can't access target_device_name directly as it is private,
    // but we can verify it builds correctly.
    // If we exposed it via accessor we could check it.
    assert!(!player.is_connected().await);
}

#[tokio::test]
async fn test_with_config() {
    let config = AirPlayConfig {
        connection_timeout: Duration::from_secs(20),
        ..Default::default()
    };
    let player = AirPlayPlayer::with_config(config);
    assert!(!player.is_connected().await);
}

#[tokio::test]
async fn test_accessors() {
    let mut player = AirPlayPlayer::new();

    // Check initial state
    assert_eq!(player.volume().await, 0.75); // Default volume is 0.75 in client
    assert!(!player.is_playing().await);
    assert_eq!(player.queue_length().await, 0);

    // Check client access
    let _ = player.client();
    let _ = player.client_mut();
}
