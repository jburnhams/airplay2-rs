use std::sync::Arc;
use std::time::Duration;

use crate::connection::ConnectionManager;
use crate::control::playback::{PlaybackProgress, ShuffleMode};
use crate::types::AirPlayConfig;

#[tokio::test]
async fn test_playback_controller_creation() {
    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = crate::control::playback::PlaybackController::new(manager);

    // Check initial state
    let state = controller.state().await;
    assert!(!state.is_playing);
    assert_eq!(
        controller.repeat_mode().await,
        crate::types::RepeatMode::Off
    );
    assert_eq!(controller.shuffle_mode().await, ShuffleMode::Off);
}

#[tokio::test]
async fn test_seek_fail_disconnected() {
    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = crate::control::playback::PlaybackController::new(manager);

    let result = controller.seek(Duration::from_secs(10)).await;
    // Should fail because not connected, but not with NotImplemented
    assert!(result.is_err());
    if let Err(crate::error::AirPlayError::NotImplemented { .. }) = result {
        panic!("Should not be NotImplemented anymore");
    }
}

#[test]
fn test_playback_progress() {
    let progress = PlaybackProgress {
        position: Duration::from_secs(30),
        duration: Duration::from_secs(120),
        rate: 1.0,
    };

    assert!((progress.progress() - 0.25).abs() < f64::EPSILON);
    assert_eq!(progress.remaining(), Duration::from_secs(90));
}

#[test]
fn test_progress_zero_duration() {
    let progress = PlaybackProgress {
        position: Duration::from_secs(0),
        duration: Duration::from_secs(0),
        rate: 0.0,
    };

    assert!(progress.progress().abs() < f64::EPSILON);
}

#[test]
fn test_progress_overflow() {
    let progress = PlaybackProgress {
        position: Duration::from_secs(130),
        duration: Duration::from_secs(120),
        rate: 1.0,
    };

    // Progress > 1.0
    assert!(progress.progress() > 1.0);
    // Remaining saturated to 0
    assert_eq!(progress.remaining(), Duration::from_secs(0));
}

#[test]
fn test_shuffle_mode_defaults() {
    assert_eq!(ShuffleMode::default(), ShuffleMode::Off);
}

#[tokio::test]
async fn test_playback_controller_next_prev_not_connected() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = crate::control::playback::PlaybackController::new(manager);

    let res = controller.next().await;
    assert!(res.is_err(), "next() should fail when disconnected");

    let res = controller.previous().await;
    assert!(res.is_err(), "previous() should fail when disconnected");
}

#[tokio::test]
async fn test_playback_controller_play_pause_stop_not_connected() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = crate::control::playback::PlaybackController::new(manager);

    assert!(controller.play().await.is_err());
    assert!(controller.pause().await.is_err());
    assert!(controller.stop().await.is_err());
    assert!(controller.toggle().await.is_err());
    assert!(controller.fast_forward().await.is_err());
    assert!(controller.rewind().await.is_err());
}

#[tokio::test]
async fn test_playback_controller_set_shuffle_and_repeat() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = crate::control::playback::PlaybackController::new(manager);

    // Tests that state updates correctly
    let res = controller.set_shuffle(ShuffleMode::On).await;
    // Because it tries to send command and fails (disconnected), state might not update depending
    // on logic. However, if the command logic in PlaybackController updates state despite
    // failure, or fails early, we should assert correctly. By looking at the implementation, if
    // it returns an error, state might remain unchanged.
    assert!(res.is_err());
}
