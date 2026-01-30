use super::rtp_receiver::*;
use crate::protocol::rtp::RtpHeader;
use crate::receiver::session::StreamParameters;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::time::Duration;

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
    let handle = tokio::spawn(async move {
        receiver.run().await
    });

    // Create a dummy RTP packet
    let header = RtpHeader::new_audio(123, 456, 789, false);
    let payload = vec![1, 2, 3, 4];

    let mut data = Vec::new();
    data.extend_from_slice(&header.encode());
    data.extend_from_slice(&payload);

    // Send it
    sender_socket.send_to(&data, receiver_addr).await.unwrap();

    // Receive and verify
    let packet = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();

    assert_eq!(packet.sequence, 123);
    assert_eq!(packet.timestamp, 456);
    assert_eq!(packet.ssrc, 789);
    assert_eq!(packet.audio_data, payload);

    handle.abort();
}
