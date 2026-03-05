use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::receiver::control_receiver::*;

struct ControlReceiverFixture {
    sender_socket: UdpSocket,
    receiver_addr: std::net::SocketAddr,
    rx: mpsc::Receiver<ControlEvent>,
    handle: JoinHandle<()>,
}

impl ControlReceiverFixture {
    async fn setup() -> Self {
        let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let receiver_addr = receiver_socket.local_addr().unwrap();
        let sender_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let (tx, rx) = mpsc::channel(1);
        let receiver = ControlReceiver::new(Arc::new(receiver_socket), tx);

        let handle = tokio::spawn(async move {
            let _ = receiver.run().await;
        });

        Self {
            sender_socket,
            receiver_addr,
            rx,
            handle,
        }
    }

    async fn send(&self, data: &[u8]) {
        self.sender_socket
            .send_to(data, self.receiver_addr)
            .await
            .unwrap();
    }

    async fn recv_timeout(&mut self, duration: Duration) -> Option<ControlEvent> {
        tokio::time::timeout(duration, self.rx.recv())
            .await
            .ok()
            .flatten()
    }
}

impl Drop for ControlReceiverFixture {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[tokio::test]
async fn test_sync_packet_reception() {
    let mut fixture = ControlReceiverFixture::setup().await;

    let data = [
        0x90, 0xD4, // Header with sync type
        0x00, 0x01, // Sequence
        0x00, 0x00, 0x01, 0x00, // RTP timestamp = 256
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // NTP timestamp = 1
        0x00, 0x00, 0x00, 0xFF, // RTP at NTP = 255
    ];

    fixture.send(&data).await;

    let event = fixture
        .recv_timeout(Duration::from_secs(1))
        .await
        .expect("Expected Sync event");

    if let ControlEvent::Sync(sync) = event {
        assert!(sync.extension);
        assert_eq!(sync.rtp_timestamp, 256);
        assert_eq!(sync.ntp_timestamp, 1);
        assert_eq!(sync.rtp_timestamp_at_ntp, 255);
    } else {
        panic!("Expected Sync event");
    }
}

#[tokio::test]
async fn test_retransmit_packet_reception() {
    let mut fixture = ControlReceiverFixture::setup().await;

    let data = [
        0x80, 0xD5, // Header with retransmit type
        0x00, 0x00, // ignored
        0x00, 0x0A, // First seq = 10
        0x00, 0x05, // Count = 5
    ];

    fixture.send(&data).await;

    let event = fixture
        .recv_timeout(Duration::from_secs(1))
        .await
        .expect("Expected RetransmitRequest event");

    if let ControlEvent::RetransmitRequest(req) = event {
        assert_eq!(req.first_seq, 10);
        assert_eq!(req.count, 5);
    } else {
        panic!("Expected RetransmitRequest event");
    }
}

#[tokio::test]
async fn test_invalid_packet_short() {
    let mut fixture = ControlReceiverFixture::setup().await;

    // Send < 8 bytes
    fixture.send(&[0x00; 5]).await;

    let result = fixture.recv_timeout(Duration::from_millis(100)).await;
    assert!(result.is_none()); // Ignored
}

#[tokio::test]
async fn test_unknown_type() {
    let mut fixture = ControlReceiverFixture::setup().await;

    // Header with unknown type (e.g., 0xFF)
    let data = [
        0x80, 0xFF, // Unknown type
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fixture.send(&data).await;

    let result = fixture.recv_timeout(Duration::from_millis(100)).await;
    assert!(result.is_none()); // Ignored
}
