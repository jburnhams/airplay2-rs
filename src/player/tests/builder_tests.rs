use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::player::{AirPlayPlayer, PlayerBuilder};
use crate::types::AirPlayConfig;

#[tokio::test]
async fn test_builder() {
    let player = PlayerBuilder::new()
        .connection_timeout(Duration::from_secs(10))
        .auto_reconnect(false)
        .device_name("Test Device")
        .build();

    assert!(!player.auto_reconnect.load(Ordering::SeqCst));
    assert!(!player.is_connected().await);
}

#[tokio::test]
async fn test_builder_defaults() {
    let player = PlayerBuilder::new().build();
    assert!(player.auto_reconnect.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_player_builder_device_name() {
    let player = PlayerBuilder::new().device_name("Bedroom Speaker").build();
    let target = player.target_device_name.read().await.clone();
    assert_eq!(target, Some("Bedroom Speaker".to_string()));
    assert!(player.auto_reconnect.load(Ordering::SeqCst));
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
