use std::time::Duration;

use airplay2::error::AirPlayError;
use airplay2::{AirPlayPlayer, PlayerBuilder};

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
}
