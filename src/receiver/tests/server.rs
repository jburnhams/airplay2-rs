use crate::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverState};

#[tokio::test]
async fn test_receiver_creation() {
    let config = ReceiverConfig::with_name("Test Receiver");
    let receiver = AirPlayReceiver::new(config);

    assert_eq!(receiver.state().await, ReceiverState::Stopped);
}

#[tokio::test]
async fn test_receiver_config_builder() {
    let config = ReceiverConfig::with_name("Kitchen")
        .port(5001)
        .latency_ms(1500);

    assert_eq!(config.name, "Kitchen");
    assert_eq!(config.port, 5001);
    assert_eq!(config.latency_ms, 1500);
}

#[tokio::test]
async fn test_event_subscription() {
    let config = ReceiverConfig::default();
    let receiver = AirPlayReceiver::new(config);

    let mut events = receiver.subscribe();

    // Events should be receivable (even if none sent yet)
    assert!(events.try_recv().is_err()); // Empty
}
