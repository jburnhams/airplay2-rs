//! Reference comparison tests
//!
//! These tests compare our receiver behavior against shairport-sync
//! to ensure compatibility.

use airplay2::testing::mock_sender::{MockSender, MockSenderConfig};
use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};

/// Compare RTSP response formats
#[tokio::test]
async fn test_options_response_format() {
    let mut receiver = AirPlayReceiver::new(ReceiverConfig::with_name("RefTest").port(0));
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();
    let port = match events.recv().await.unwrap() {
        ReceiverEvent::Started { port, .. } => port,
        _ => panic!("Expected Started event"),
    };

    let mut sender = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });
    sender.connect().await.unwrap();

    let response = sender.options().await.unwrap();
    assert_eq!(response.status.0, 200);

    let public = response.headers.get("Public").expect("Missing Public header");
    let expected_methods = [
        "ANNOUNCE", "SETUP", "RECORD", "PAUSE", "FLUSH",
        "TEARDOWN", "OPTIONS", "GET_PARAMETER", "SET_PARAMETER"
    ];

    for method in expected_methods {
        assert!(public.contains(method), "Missing method: {}", method);
    }

    receiver.stop().await.unwrap();
}

/// Compare audio latency behavior
#[tokio::test]
async fn test_audio_latency_header() {
    let mut receiver = AirPlayReceiver::new(ReceiverConfig::with_name("RefTestLatency").port(0));
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();
    let port = match events.recv().await.unwrap() {
        ReceiverEvent::Started { port, .. } => port,
        _ => panic!("Expected Started event"),
    };

    let mut sender = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });
    sender.connect().await.unwrap();
    sender.options().await.unwrap();
    sender.announce().await.unwrap();
    sender.setup().await.unwrap();

    let record_resp = sender.record().await.unwrap();
    assert_eq!(record_resp.status.0, 200);

    // Check if Audio-Latency header is present (shairport-sync provides it)
    assert!(record_resp.headers.get("Audio-Latency").is_some());

    receiver.stop().await.unwrap();
}
