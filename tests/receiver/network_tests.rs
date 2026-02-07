//! Network simulation tests for AirPlay receiver

use airplay2::testing::mock_sender::{MockSender, MockSenderConfig};
use airplay2::testing::network_sim::NetworkSimulator;
use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};
use std::time::Duration;

/// Test streaming with packet loss and jitter
#[tokio::test]
async fn test_streaming_with_network_issues() {
    // Start receiver
    let mut receiver = AirPlayReceiver::new(
        ReceiverConfig::with_name("NetworkTest").port(0)
    );
    let mut events = receiver.subscribe();
    receiver.start().await.unwrap();

    // Get actual port
    let event = events.recv().await.unwrap();
    let port = match event {
        ReceiverEvent::Started { port, .. } => port,
        _ => panic!("Expected Started event"),
    };

    // Create sender with network simulation
    let mut sender = MockSender::new(MockSenderConfig {
        receiver_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        ..Default::default()
    });

    // Configure poor wifi conditions (loss + jitter)
    let mut sim = NetworkSimulator::good_wifi();
    sim.loss_rate = 0.05; // 5% loss
    sim.jitter_ms = 20;   // 20ms jitter
    sender.set_network_conditions(sim);

    // Connect and negotiate
    sender.connect().await.unwrap();
    sender.options().await.unwrap();
    sender.announce().await.unwrap();
    sender.setup().await.unwrap();
    sender.record().await.unwrap();

    // Send audio for 2 seconds (approx 250 packets)
    // 250 packets * 5% loss ~= 12 packets dropped
    for _ in 0..250 {
        // Send silence
        sender.send_audio(&vec![0u8; 1408]).await.unwrap();
        // Send faster than real-time to fill buffer
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Wait a bit for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify session is still alive (no teardown from server side)
    // We expect the receiver to handle loss gracefully (concealment) and not crash

    // Check if we can still communicate
    let response = sender.set_volume(-20.0).await;
    assert!(response.is_ok(), "Session should remain active despite packet loss");

    sender.teardown().await.unwrap();
    receiver.stop().await.unwrap();
}
