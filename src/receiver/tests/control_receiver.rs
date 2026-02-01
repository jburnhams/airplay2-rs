use crate::receiver::control_receiver::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_sync_packet_reception() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let (tx, mut rx) = mpsc::channel(1);
    let receiver = ControlReceiver::new(Arc::new(receiver_socket), tx);

    let handle = tokio::spawn(async move { receiver.run().await });

    let data = [
        0x90, 0xD4, // Header with sync type
        0x00, 0x01, // Sequence
        0x00, 0x00, 0x01, 0x00, // RTP timestamp = 256
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // NTP timestamp = 1
        0x00, 0x00, 0x00, 0xFF, // RTP at NTP = 255
    ];

    sender_socket.send_to(&data, receiver_addr).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();

    if let ControlEvent::Sync(sync) = event {
        assert!(sync.extension);
        assert_eq!(sync.rtp_timestamp, 256);
        assert_eq!(sync.ntp_timestamp, 1);
        assert_eq!(sync.rtp_timestamp_at_ntp, 255);
    } else {
        panic!("Expected Sync event");
    }

    handle.abort();
}

#[tokio::test]
async fn test_retransmit_packet_reception() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let (tx, mut rx) = mpsc::channel(1);
    let receiver = ControlReceiver::new(Arc::new(receiver_socket), tx);

    let handle = tokio::spawn(async move { receiver.run().await });

    let data = [
        0x80, 0xD5, // Header with retransmit type
        0x00, 0x00, // ignored
        0x00, 0x0A, // First seq = 10
        0x00, 0x05, // Count = 5
    ];

    sender_socket.send_to(&data, receiver_addr).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();

    if let ControlEvent::RetransmitRequest(req) = event {
        assert_eq!(req.first_seq, 10);
        assert_eq!(req.count, 5);
    } else {
        panic!("Expected RetransmitRequest event");
    }

    handle.abort();
}

#[tokio::test]
async fn test_invalid_packet_short() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let receiver = ControlReceiver::new(Arc::new(receiver_socket), tx);
    let handle = tokio::spawn(async move { receiver.run().await });

    // Send < 8 bytes
    sender_socket
        .send_to(&[0x00; 5], receiver_addr)
        .await
        .unwrap();

    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err()); // Timeout, ignored

    handle.abort();
}

#[tokio::test]
async fn test_unknown_type() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let receiver = ControlReceiver::new(Arc::new(receiver_socket), tx);
    let handle = tokio::spawn(async move { receiver.run().await });

    // Header with unknown type (e.g., 0xFF)
    let data = [
        0x80, 0xFF, // Unknown type
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    sender_socket.send_to(&data, receiver_addr).await.unwrap();

    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err()); // Timeout, ignored

    handle.abort();
}
