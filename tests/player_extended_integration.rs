use std::time::Duration;

use airplay2::{AirPlayError, quick_connect, quick_connect_to, quick_play};

mod common;

#[tokio::test]
async fn test_quick_connect_timeout() {
    common::init_logging();

    // With a short timeout of 5 seconds (internal to quick_connect) and no devices mocked,
    // this should time out. Since `quick_connect` uses the default scan (which has hardcoded 5s
    // timeout internally), we expect it to return DeviceNotFound.

    // Actually, `quick_connect()` currently has hardcoded 5s timeout in `src/player/mod.rs`
    let result = tokio::time::timeout(Duration::from_secs(6), quick_connect()).await;

    match result {
        Ok(Err(AirPlayError::DeviceNotFound { .. })) => {
            // Expected
        }
        Ok(Ok(_)) => panic!("Expected DeviceNotFound error, got Ok"),
        Ok(Err(e)) => panic!("Expected DeviceNotFound error, got other error: {:?}", e),
        Err(_) => panic!("Test itself timed out waiting for quick_connect"),
    }
}

#[tokio::test]
async fn test_quick_connect_to_timeout() {
    common::init_logging();

    let result = tokio::time::timeout(
        Duration::from_secs(6),
        quick_connect_to("NonExistentDevice"),
    )
    .await;

    match result {
        Ok(Err(AirPlayError::DeviceNotFound { device_id })) => {
            assert_eq!(device_id, "NonExistentDevice");
        }
        Ok(Ok(_)) => panic!("Expected DeviceNotFound error, got Ok"),
        Ok(Err(e)) => panic!("Expected DeviceNotFound error, got other error: {:?}", e),
        Err(_) => panic!("Test itself timed out waiting for quick_connect_to"),
    }
}

#[tokio::test]
async fn test_quick_play_timeout() {
    common::init_logging();

    let tracks = vec![(
        "http://test.com/audio.mp3".to_string(),
        "Title".to_string(),
        "Artist".to_string(),
    )];

    let result = tokio::time::timeout(Duration::from_secs(6), quick_play(tracks)).await;

    match result {
        Ok(Err(AirPlayError::DeviceNotFound { .. })) => {
            // Expected
        }
        Ok(Ok(_)) => panic!("Expected DeviceNotFound error, got Ok"),
        Ok(Err(e)) => panic!("Expected DeviceNotFound error, got other error: {:?}", e),
        Err(_) => panic!("Test itself timed out waiting for quick_play"),
    }
}
