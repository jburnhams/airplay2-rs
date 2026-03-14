use std::time::Duration;

use airplay2::protocol::rtp::ntp_client::NtpClient;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_ntp_client_against_mock_server() {
    // Start a mock NTP server on a random local port
    let mock_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = mock_server.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let mut buf = [0u8; 48];
        // Wait for request
        let (len, peer) = mock_server.recv_from(&mut buf).await.unwrap();
        assert_eq!(len, 48);

        // Verify request format (Mode 3 = Client)
        assert_eq!(buf[0] & 0x07, 3);

        // Copy client's transmit timestamp (bytes 40-47) into our originate timestamp (bytes 24-31)
        // This is crucial for the client's matching logic.
        let mut resp = [0u8; 48];
        resp[0] = 0x24; // Mode 4 (Server)

        // Copy the originate timestamp from the request's transmit timestamp
        resp[24..32].copy_from_slice(&buf[40..48]);

        // Provide some arbitrary valid timestamps for receive and transmit
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // NTP epoch is 1900, UNIX is 1970. Add 70 years of seconds.
        let ntp_secs = current_time + 2208988800;
        let time_bytes = (ntp_secs as u32).to_be_bytes();

        // Receive Timestamp (bytes 32-39)
        resp[32..36].copy_from_slice(&time_bytes);
        // Transmit Timestamp (bytes 40-47)
        resp[40..44].copy_from_slice(&time_bytes);

        mock_server.send_to(&resp, peer).await.unwrap();
    });

    let client = NtpClient::new(server_addr.to_string(), Duration::from_secs(5));
    let offset_result = client.get_offset().await;

    assert!(
        offset_result.is_ok(),
        "NTP client failed to get offset from mock server: {:?}",
        offset_result.err()
    );

    let offset = offset_result.unwrap();
    println!("Got NTP offset: {} us", offset);
    // As long as the request returns a valid offset, we know packet encoding/decoding works
    // correctly.

    server_handle.await.unwrap();
}
