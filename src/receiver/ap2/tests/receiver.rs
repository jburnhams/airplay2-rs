use crate::receiver::ap2::config::Ap2Config;
use crate::receiver::ap2::receiver::{AirPlay2Receiver, ReceiverBuilder, ReceiverState};

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
