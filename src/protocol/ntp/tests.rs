use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use super::client::{NtpClient, NtpPacket, NTP_PACKET_SIZE};
use crate::protocol::rtp::timing::NtpTimestamp;

#[test]
fn test_ntp_packet_encode_decode() {
    let mut packet = NtpPacket::new_client_request();
    packet.stratum = 2;
    packet.poll = 4;
    packet.precision = -6;
    packet.root_delay = 1000;
    packet.root_dispersion = 2000;
    packet.reference_id = 12345;
    packet.reference_timestamp = NtpTimestamp { seconds: 1, fraction: 2 };
    packet.origin_timestamp = NtpTimestamp { seconds: 3, fraction: 4 };
    packet.receive_timestamp = NtpTimestamp { seconds: 5, fraction: 6 };
    // transmit_timestamp is set by new_client_request()

    let encoded = packet.encode();
    assert_eq!(encoded.len(), NTP_PACKET_SIZE);

    let decoded = NtpPacket::decode(&encoded).expect("Failed to decode NTP packet");

    assert_eq!(decoded.li_vn_mode, packet.li_vn_mode);
    assert_eq!(decoded.stratum, packet.stratum);
    assert_eq!(decoded.poll, packet.poll);
    assert_eq!(decoded.precision, packet.precision);
    assert_eq!(decoded.root_delay, packet.root_delay);
    assert_eq!(decoded.root_dispersion, packet.root_dispersion);
    assert_eq!(decoded.reference_id, packet.reference_id);

    assert_eq!(decoded.reference_timestamp.seconds, packet.reference_timestamp.seconds);
    assert_eq!(decoded.reference_timestamp.fraction, packet.reference_timestamp.fraction);

    assert_eq!(decoded.origin_timestamp.seconds, packet.origin_timestamp.seconds);
    assert_eq!(decoded.origin_timestamp.fraction, packet.origin_timestamp.fraction);

    assert_eq!(decoded.receive_timestamp.seconds, packet.receive_timestamp.seconds);
    assert_eq!(decoded.receive_timestamp.fraction, packet.receive_timestamp.fraction);

    assert_eq!(decoded.transmit_timestamp.seconds, packet.transmit_timestamp.seconds);
    assert_eq!(decoded.transmit_timestamp.fraction, packet.transmit_timestamp.fraction);
}

#[tokio::test]
async fn test_ntp_client_offset_calculation() {
    // We'll create a dummy server to respond to our client's request
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let client = NtpClient::new().await.unwrap();

    // Spawn the server task to receive and respond
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        let (len, src) = server_socket.recv_from(&mut buf).await.unwrap();

        let req = NtpPacket::decode(&buf[..len]).unwrap();

        let mut resp = NtpPacket::default();
        resp.li_vn_mode = 0x24; // Server mode

        // Let's set the origin timestamp to match the client's transmit timestamp
        resp.origin_timestamp = req.transmit_timestamp;

        // Create an artificial offset of 500,000 microseconds (0.5s) ahead
        let mut t2 = req.transmit_timestamp;
        t2.fraction = t2.fraction.wrapping_add(0x8000_0000); // add 0.5s

        let mut t3 = t2;
        t3.fraction = t3.fraction.wrapping_add(0x1000_0000); // add some small processing time

        resp.receive_timestamp = t2;
        resp.transmit_timestamp = t3;

        server_socket.send_to(&resp.encode(), src).await.unwrap();
    });

    let (offset, rtt) = client.get_offset(server_addr, Duration::from_secs(2)).await.unwrap();

    // The offset should be close to 500,000us (0.5s). Due to execution time it won't be exact, but should be > 0
    assert!(offset > 0, "Offset should be positive, got {}", offset);

    // Rtt should be relatively small in a local loopback
    assert!(rtt < 100_000, "RTT is unusually high: {}", rtt);
}
