//! Protocol conformance tests for AirPlay receiver

use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};
use airplay2::testing::mock_sender::{MockSender, MockSenderConfig};
use std::time::Duration;

/// Test complete session negotiation
#[tokio::test]
async fn test_complete_session() {
    // Start receiver
    let mut receiver = AirPlayReceiver::new(ReceiverConfig::with_name("Test").port(0));
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();

    // Get actual port
    let event = events.recv().await.unwrap();
    let port = match event {
        ReceiverEvent::Started { port, .. } => port,
        _ => panic!("Expected Started event"),
    };

    // Create sender
    let mut sender = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });

    // Connect and negotiate
    sender.connect().await.unwrap();

    let options = sender.options().await.unwrap();
    assert_eq!(options.status.0, 200);

    let announce = sender.announce().await.unwrap();
    assert_eq!(announce.status.0, 200);

    let setup = sender.setup().await.unwrap();
    assert_eq!(setup.status.0, 200);
    assert!(setup.headers.get("Transport").is_some());
    assert!(setup.headers.get("Session").is_some());

    let record = sender.record().await.unwrap();
    assert_eq!(record.status.0, 200);

    // Send some audio
    for _ in 0..10 {
        sender.send_audio(&vec![0u8; 1408]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(8)).await;
    }

    let teardown = sender.teardown().await.unwrap();
    assert_eq!(teardown.status.0, 200);

    receiver.stop().await.unwrap();
}

/// Test volume control
#[tokio::test]
async fn test_volume_control() {
    let mut receiver = AirPlayReceiver::new(ReceiverConfig::with_name("Test").port(0));
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();

    let event = events.recv().await.unwrap();
    let port = match event {
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

    // Set volume
    let response = sender.set_volume(-15.0).await.unwrap();
    assert_eq!(response.status.0, 200);

    // Check event
    let result = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            match events.recv().await {
                Ok(ReceiverEvent::VolumeChanged { db, .. }) => {
                    if (db - -15.0).abs() < 0.001 {
                        return true;
                    }
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await;

    assert!(result.unwrap_or(false), "VolumeChanged event not received");

    sender.teardown().await.unwrap();
    receiver.stop().await.unwrap();
}

/// Test session preemption
#[tokio::test]
async fn test_session_preemption() {
    let mut receiver = AirPlayReceiver::new(ReceiverConfig::with_name("Test").port(0));
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();

    let event = events.recv().await.unwrap();
    let port = match event {
        ReceiverEvent::Started { port, .. } => port,
        _ => panic!("Expected Started event"),
    };

    // First sender connects
    let mut sender1 = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });

    sender1.connect().await.unwrap();
    sender1.options().await.unwrap();
    sender1.announce().await.unwrap();
    sender1.setup().await.unwrap();
    sender1.record().await.unwrap();

    // Second sender preempts
    let mut sender2 = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });

    sender2.connect().await.unwrap();
    let response = sender2.options().await.unwrap();
    assert_eq!(response.status.0, 200); // Should succeed with preemption

    receiver.stop().await.unwrap();
}
