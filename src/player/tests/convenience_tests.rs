use crate::error::AirPlayError;
use crate::player::{quick_connect, quick_connect_to, quick_play};

#[tokio::test]
async fn test_quick_play_failure() {
    let tracks = vec![(
        "http://example.com/1.mp3".to_string(),
        "Title".to_string(),
        "Artist".to_string(),
    )];

    // Should fail because scan won't find a device in our mocked environment
    let result = quick_play(tracks).await;
    assert!(matches!(result, Err(AirPlayError::DeviceNotFound { .. })));
}

#[tokio::test]
async fn test_quick_connect_failure() {
    // Should fail because scan won't find a device
    let result = quick_connect().await;
    assert!(matches!(result, Err(AirPlayError::DeviceNotFound { .. })));
}

#[tokio::test]
async fn test_quick_connect_to_failure() {
    // Should fail because scan won't find the named device
    let result = quick_connect_to("NonExistentDevice").await;
    assert!(matches!(result, Err(AirPlayError::DeviceNotFound { .. })));
}
