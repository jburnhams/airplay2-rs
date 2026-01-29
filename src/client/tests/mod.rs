use super::*;
use crate::types::TrackInfo;

#[tokio::test]
async fn test_client_creation() {
    let client = AirPlayClient::default_client();
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_queue_operations() {
    let client = AirPlayClient::default_client();

    let track = TrackInfo {
        url: "http://example.com/stream".to_string(),
        title: "Test Track".to_string(),
        artist: "Test Artist".to_string(),
        album: None,
        duration_secs: Some(180.0),
        artwork_url: None,
        ..Default::default()
    };

    let id = client.add_to_queue(track.clone()).await;
    let queue = client.queue().await;

    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].track.title, track.title);

    client.remove_from_queue(id).await;
    assert!(client.queue().await.is_empty());
}

#[tokio::test]
async fn test_volume_defaults() {
    let client = AirPlayClient::default_client();
    // Default volume is 0.75
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);
}

mod protocol_tests;
mod raop_session_tests;
mod unified_tests;
