use crate::client::AirPlayClient;
use crate::state::ClientEvent;
use crate::types::{AirPlayConfig, TimingProtocol, TrackInfo};

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
    // Default volume is 0.75 in VolumeController
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);

    // Check state consistency
    let state = client.state().await;
    assert!(
        (state.volume - 0.75).abs() < f32::EPSILON,
        "State volume {} does not match default 0.75",
        state.volume
    );
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

// --- PTP timing config tests ---

#[tokio::test]
async fn test_client_with_ptp_config() {
    let config = AirPlayConfig {
        timing_protocol: TimingProtocol::Ptp,
        ..Default::default()
    };
    let client = AirPlayClient::new(config);
    // Client should be created successfully with PTP config
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_client_with_auto_timing_config() {
    let config = AirPlayConfig::builder()
        .timing_protocol(TimingProtocol::Auto)
        .build();
    let client = AirPlayClient::new(config);
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_client_with_ntp_timing_config() {
    let config = AirPlayConfig::builder()
        .timing_protocol(TimingProtocol::Ntp)
        .build();
    let client = AirPlayClient::new(config);
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_client_connect_fails_without_device_ptp() {
    // Connecting to a non-existent device should fail regardless of timing protocol
    let config = AirPlayConfig {
        timing_protocol: TimingProtocol::Ptp,
        ..Default::default()
    };
    let client = AirPlayClient::new(config);

    let device = crate::types::AirPlayDevice {
        id: "fake".to_string(),
        name: "Fake HomePod".to_string(),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 1, // Non-existent service
        capabilities: crate::types::DeviceCapabilities {
            supports_ptp: true,
            airplay2: true,
            supports_audio: true,
            ..Default::default()
        },
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    let result = client.connect(&device).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_play_url_disconnected() {
    let client = AirPlayClient::default_client();
    let result = client.play_url("http://example.com/audio.mp3").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stop_disconnected() {
    let client = AirPlayClient::default_client();
    let result = client.stop().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_set_shuffle_disconnected() {
    let client = AirPlayClient::default_client();
    let result = client.set_shuffle(true).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_set_repeat_disconnected() {
    let client = AirPlayClient::default_client();
    let result = client.set_repeat(crate::types::RepeatMode::One).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_set_metadata_disconnected() {
    let client = AirPlayClient::default_client();
    let metadata = crate::protocol::daap::TrackMetadata {
        title: Some("Test".to_string()),
        ..Default::default()
    };
    let result = client.set_metadata(metadata).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_set_progress_disconnected() {
    let client = AirPlayClient::default_client();
    let progress = crate::protocol::daap::DmapProgress::new(0, 0, 0);
    let result = client.set_progress(progress).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_playback_info_disconnected() {
    let client = AirPlayClient::default_client();
    let result = client.get_playback_info().await;
    assert!(result.is_err());
}
