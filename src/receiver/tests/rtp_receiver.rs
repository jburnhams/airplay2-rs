use crate::protocol::rtp::RtpHeader;
use crate::receiver::rtp_receiver::*;
use crate::receiver::session::StreamParameters;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

#[test]
fn test_audio_decryptor() {
    let key = [0x01; 16];
    let iv = [0x02; 16];
    let decryptor = AudioDecryptor::new(key, iv);

    // Test with less than one block (unencrypted)
    let short_data = [0x03; 10];
    let result = decryptor.decrypt(&short_data).unwrap();
    assert_eq!(result, short_data);
}

#[test]
fn test_audio_decryptor_full_block() {
    let key = [0x00; 16];
    let iv = [0x00; 16];
    let decryptor = AudioDecryptor::new(key, iv);

    // Test logic structure with multi-block
    let data = vec![0u8; 32];
    let result = decryptor.decrypt(&data).unwrap();
    assert_eq!(result.len(), 32);
}

#[tokio::test]
async fn test_packet_reception() {
    // Setup UDP sockets
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let (tx, mut rx) = mpsc::channel(1);

    let params = StreamParameters {
        aes_key: None,
        aes_iv: None,
        ..Default::default()
    };

    let receiver = RtpAudioReceiver::new(Arc::new(receiver_socket), params, tx);

    // Start receiver in background
    let handle = tokio::spawn(async move { receiver.run().await });

    // Create a dummy RTP packet
    let header = RtpHeader::new_audio(123, 456, 789, false);
    let payload = vec![1, 2, 3, 4];

    let mut data = Vec::new();
    data.extend_from_slice(&header.encode());
    data.extend_from_slice(&payload);

    // Send it
    sender_socket.send_to(&data, receiver_addr).await.unwrap();

    // Receive and verify
    let packet = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(packet.sequence, 123);
    assert_eq!(packet.timestamp, 456);
    assert_eq!(packet.ssrc, 789);
    assert_eq!(packet.audio_data, payload);

    handle.abort();
}

#[tokio::test]
async fn test_packet_reception_invalid_payload_type() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let params = StreamParameters::default();
    let receiver = RtpAudioReceiver::new(Arc::new(receiver_socket), params, tx);
    let handle = tokio::spawn(async move { receiver.run().await });

    // Header with wrong payload type (e.g. 0)
    let header = RtpHeader::new_audio(1, 0, 0, false);
    // Force payload type to something else by encoding then modifying bytes
    let mut encoded = header.encode();
    encoded[1] = 0x00; // Marker=0, PT=0

    sender_socket
        .send_to(&encoded, receiver_addr)
        .await
        .unwrap();

    // Should NOT receive anything
    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err()); // Timeout

    handle.abort();
}

#[tokio::test]
async fn test_packet_reception_short_packet() {
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let receiver_addr = receiver_socket.local_addr().unwrap();
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let params = StreamParameters::default();
    let receiver = RtpAudioReceiver::new(Arc::new(receiver_socket), params, tx);
    let handle = tokio::spawn(async move { receiver.run().await });

    // Send 5 bytes (less than header)
    sender_socket
        .send_to(&[1, 2, 3, 4, 5], receiver_addr)
        .await
        .unwrap();

    // Should NOT receive anything
    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err());

    handle.abort();
}
