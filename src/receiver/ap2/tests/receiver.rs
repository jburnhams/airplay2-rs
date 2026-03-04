use std::time::Duration;

use tokio::net::TcpStream;

use crate::receiver::ap2::{
    AirPlay2Receiver, Ap2Config, ReceiverBuilder, ReceiverEvent, ReceiverState,
};

#[tokio::test]
async fn test_receiver_creation() {
    let config = Ap2Config::new("Test Speaker");
    let receiver = AirPlay2Receiver::new(config).unwrap();

    assert_eq!(receiver.state().await, ReceiverState::Stopped);
}

#[tokio::test]
async fn test_builder() {
    let receiver = ReceiverBuilder::new("Test Speaker")
        .password("secret")
        .port(7001)
        .build()
        .unwrap();

    assert_eq!(receiver.config().server_port, 7001);
    assert!(receiver.config().password.is_some());
}

#[tokio::test]
async fn test_start_stop() {
    let mut receiver = ReceiverBuilder::new("Test Speaker")
        .port(7002)
        .build()
        .unwrap();

    assert_eq!(receiver.state().await, ReceiverState::Stopped);

    let mut events = receiver.subscribe();

    receiver.start().await.unwrap();

    assert_eq!(receiver.state().await, ReceiverState::Running);

    let event = events.recv().await.unwrap();
    assert!(matches!(event, ReceiverEvent::Started));

    receiver.stop().await.unwrap();

    assert_eq!(receiver.state().await, ReceiverState::Stopped);

    let event = events.recv().await.unwrap();
    assert!(matches!(event, ReceiverEvent::Stopped));
}

#[tokio::test]
async fn test_accept_connection() {
    let mut receiver = ReceiverBuilder::new("Test Speaker")
        .port(7003)
        .build()
        .unwrap();

    let mut events = receiver.subscribe();

    receiver.start().await.unwrap();

    // Consume Started event
    let event = events.recv().await.unwrap();
    assert!(matches!(event, ReceiverEvent::Started));

    // Connect to the receiver
    let stream = TcpStream::connect("127.0.0.1:7003").await.unwrap();

    // Verify Connected event is emitted
    let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, ReceiverEvent::Connected { .. }));

    // Clean up
    receiver.stop().await.unwrap();
    drop(stream);
}
