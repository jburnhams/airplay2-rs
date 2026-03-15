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

    client.clear_queue().await;
    assert!(client.queue().await.is_empty());
}

#[tokio::test]
async fn test_volume_defaults() {
    let client = AirPlayClient::default_client();
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);

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
    assert!((client.volume().await - 0.75).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_event_subscription() {
    let client = AirPlayClient::default_client();
    let mut rx = client.subscribe_events();

    let track = TrackInfo::default();
    client.add_to_queue(track).await;

    let event = rx.recv().await;
    assert!(event.is_ok());
    match event.unwrap() {
        ClientEvent::QueueUpdated { length } => assert_eq!(length, 1),
        _ => panic!("Expected QueueUpdated event"),
    }
}

#[tokio::test]
async fn test_client_with_ptp_config() {
    let config = AirPlayConfig {
        timing_protocol: TimingProtocol::Ptp,
        ..Default::default()
    };
    let client = AirPlayClient::new(config);
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
        port: 1,
        capabilities: crate::types::DeviceCapabilities {
            supports_ptp: true,
            airplay2: true,
            supports_audio: true,
            ..Default::default()
        },
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
        last_seen: None,
    };

    let result = client.connect(&device).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_play_url_fails_without_connection() {
    let client = AirPlayClient::default_client();
    let res = client.play_url("http://example.com/audio.mp3").await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));
}

#[tokio::test]
async fn test_volume_controls_fail_without_connection() {
    let client = AirPlayClient::default_client();

    let res = client.volume_up().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.volume_down().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.mute().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.unmute().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.toggle_mute().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));
}

#[tokio::test]
async fn test_playback_controls_fail_without_connection() {
    let client = AirPlayClient::default_client();

    let res = client.play().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.pause().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.stop().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.next().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.previous().await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));

    let res = client.seek(std::time::Duration::from_secs(10)).await;
    assert!(matches!(
        res,
        Err(crate::error::AirPlayError::Disconnected { .. })
    ));
}

#[tokio::test]
async fn test_forget_device() {
    let client = AirPlayClient::default_client();
    let res = client.forget_device("some_device_id").await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_with_pairing_storage() {
    let client = AirPlayClient::default_client();
    struct DummyStorage;
    #[async_trait::async_trait]
    impl crate::protocol::pairing::PairingStorage for DummyStorage {
        async fn load(&self, _: &str) -> Option<crate::protocol::pairing::PairingKeys> {
            None
        }
        async fn save(
            &mut self,
            _: &str,
            _: &crate::protocol::pairing::PairingKeys,
        ) -> Result<(), crate::protocol::pairing::storage::StorageError> {
            Ok(())
        }
        async fn remove(
            &mut self,
            _: &str,
        ) -> Result<(), crate::protocol::pairing::storage::StorageError> {
            Ok(())
        }
        async fn list_devices(&self) -> Vec<String> {
            Vec::new()
        }
    }
    let client = client.with_pairing_storage(Box::new(DummyStorage));
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_disconnect_emits_event_when_connected() {
    let client = AirPlayClient::default_client();
    let res = client.disconnect().await;
    assert!(res.is_ok());
}
