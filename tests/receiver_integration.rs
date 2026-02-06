use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};
use std::time::Duration;

#[tokio::test]
async fn test_receiver_start_stop() {
    let config = ReceiverConfig::with_name("Integration Test").port(0); // Auto-assign port

    let mut receiver = AirPlayReceiver::new(config);
    let mut events = receiver.subscribe();

    // Start
    receiver.start().await.unwrap();

    // Wait for started event
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();

    match event {
        ReceiverEvent::Started { port, .. } => {
            assert!(port > 0);
        }
        _ => panic!("Expected Started event, got {:?}", event),
    }

    // Stop
    receiver.stop().await.unwrap();

    // Wait for stopped event
    // The events.recv() might return Started again if multiple were queued or something,
    // so we should look for Stopped.
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            ReceiverEvent::Stopped => break,
            ReceiverEvent::Started { .. } => continue, // Ignore extra started
            _ => panic!("Expected Stopped event, got {:?}", event),
        }
    }
}
