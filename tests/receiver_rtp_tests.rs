use airplay2::receiver::rtp_receiver::RtpAudioReceiver;
use airplay2::receiver::session::StreamParameters;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_audio_packet_reception() {
    // Create receiver socket
    let receiver_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let receiver_addr = receiver_socket.local_addr().unwrap();

    // Create sender socket
    let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Setup receiver
    let (tx, mut rx) = mpsc::channel(16);
    let params = StreamParameters::default();
    let audio_receiver = RtpAudioReceiver::new(receiver_socket, params, tx);

    // Start receiver
    let handle = tokio::spawn(async move { audio_receiver.run().await });

    // Send a valid RTP packet
    let rtp_packet = build_test_rtp_packet(1, 0, &[0xAB; 100]);
    sender_socket
        .send_to(&rtp_packet, receiver_addr)
        .await
        .unwrap();

    // Receive and verify
    let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(received.sequence, 1);
    assert_eq!(received.timestamp, 0);

    handle.abort();
}

fn build_test_rtp_packet(seq: u16, timestamp: u32, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![
        0x80,
        0x60, // V=2, PT=96
        (seq >> 8) as u8,
        seq as u8,
        (timestamp >> 24) as u8,
        (timestamp >> 16) as u8,
        (timestamp >> 8) as u8,
        timestamp as u8,
        0x12,
        0x34,
        0x56,
        0x78, // SSRC
    ];
    packet.extend_from_slice(payload);
    packet
}
