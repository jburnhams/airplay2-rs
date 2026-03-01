use std::time::Duration;

use super::*;
use crate::error::AirPlayError;

#[tokio::test]
async fn test_player_creation() {
    let player = AirPlayPlayer::new();
    assert!(!player.is_connected().await);
    assert!(player.device().await.is_none());
    assert_eq!(player.queue_length().await, 0);
    assert!(!player.is_playing().await);
}

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

    // Check volume (default depends on implementation but should be valid f32)
    let vol = player.volume().await;
    assert!((0.0..=1.0).contains(&vol));

    assert!(!player.is_playing().await);
    assert_eq!(player.queue_length().await, 0);

    // Check playback state
    let state = player.playback_state().await;
    assert!(!state.is_playing);

    // Check client access
    let _ = player.client();
    let _ = player.client_mut();
}

#[tokio::test]
async fn test_disconnected_errors() {
    let player = AirPlayPlayer::new();

    // play_track checks connection
    let res = player
        .play_track("http://example.com/1.mp3", "Title", "Artist")
        .await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // play checks connection
    let res = player.play().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // pause checks connection
    let res = player.pause().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // stop checks connection
    let res = player.stop().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // set_volume checks connection
    let res = player.set_volume(0.5).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // mute/unmute checks connection
    let res = player.mute().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.unmute().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // seek checks connection
    let res = player.seek(10.0).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[tokio::test]
async fn test_queue_manipulation_when_disconnected() {
    let player = AirPlayPlayer::new();

    // Adding to queue should work even if disconnected as queue is local
    let track = TrackInfo::new("http://example.com/1.mp3", "Title", "Artist");
    player.client().add_to_queue(track.clone()).await;

    assert_eq!(player.queue_length().await, 1);

    // Verify track in queue
    let queue = player.client().queue().await;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].track.url, "http://example.com/1.mp3");

    // But playing tracks (which attempts to start playback) will fail
    let res = player
        .play_tracks(vec![(
            "http://example.com/2.mp3".to_string(),
            "Title 2".to_string(),
            "Artist 2".to_string(),
        )])
        .await;

    // play_tracks clears queue, adds tracks, then calls play_url.
    // It should fail at play_url step.
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    // Queue should now contain the new track (even if play failed, it was added)
    assert_eq!(player.queue_length().await, 1);
    let queue = player.client().queue().await;
    assert_eq!(queue[0].track.url, "http://example.com/2.mp3");

    // Clear queue
    player.client().clear_queue().await;
    assert_eq!(player.queue_length().await, 0);
}

#[tokio::test]
async fn test_repeat_modes_disconnected() {
    let player = AirPlayPlayer::new();

    let res = player.repeat_off().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.repeat_one().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.repeat_all().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[tokio::test]
async fn test_shuffle_disconnected() {
    let player = AirPlayPlayer::new();

    let res = player.shuffle_on().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = player.shuffle_off().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[cfg(feature = "decoders")]
#[tokio::test]
async fn test_play_file_disconnected() {
    let mut player = AirPlayPlayer::new();

    // Attempting to play a non-existent file should fail with IoError or Disconnected
    // We expect IoError first because it tries to open the file before checking connection
    let res = player.play_file("non_existent_file.mp3").await;
    assert!(matches!(res, Err(AirPlayError::IoError { .. })));
}

#[tokio::test]
async fn test_play_tracks_disconnected() {
    let player = AirPlayPlayer::new();
    let res = player.play_tracks(vec![]).await;

    // An empty track list falls back to client.play(), which requires connection
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[tokio::test]
async fn test_target_device_name() {
    let player = AirPlayPlayer::new();
    player
        .set_target_device_name(Some("HomePod".to_string()))
        .await;

    let target = player.target_device_name.read().await.clone();
    assert_eq!(target, Some("HomePod".to_string()));
}
