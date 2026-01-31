use crate::client::AirPlayClient;
use crate::state::ClientEvent;
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
async fn test_queue_shuffle_clear() {
    let client = AirPlayClient::default_client();
    let track1 = TrackInfo {
        title: "Track 1".to_string(),
        ..Default::default()
    };
    let track2 = TrackInfo {
        title: "Track 2".to_string(),
        ..Default::default()
    };

    client.add_to_queue(track1).await;
    client.add_to_queue(track2).await;

    assert_eq!(client.queue().await.len(), 2);

    // Testing shuffle toggle logic (no network needed for local queue state shuffle flag)
    // Note: client.set_shuffle() calls playback.set_shuffle() which calls network.
    // So we can't test set_shuffle() fully without connection mocking.

    // But we can test clear_queue
    client.clear_queue().await;
    assert!(client.queue().await.is_empty());
}

#[tokio::test]
async fn test_volume_defaults() {
    let client = AirPlayClient::default_client();
    // Default volume is 0.75
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_volume_set_fails_without_connection() {
    let client = AirPlayClient::default_client();
    let result = client.set_volume(0.5).await;
    assert!(result.is_err());
    // Volume should not have changed because set failed
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_event_subscription() {
    let client = AirPlayClient::default_client();
    let mut rx = client.subscribe_events();

    // Trigger an event that doesn't require network (e.g. queue update)
    let track = TrackInfo::default();
    client.add_to_queue(track).await;

    // We should receive an event
    let event = rx.recv().await;
    assert!(event.is_ok());
    match event.unwrap() {
        ClientEvent::QueueUpdated { length } => assert_eq!(length, 1),
        _ => panic!("Expected QueueUpdated event"),
    }
}
