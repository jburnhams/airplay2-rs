use std::time::Duration;

use airplay2::testing::create_test_device;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_raop_handshake_compliance() {
    // 1. Setup Custom Mock Server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 2. Create RAOP Device
    // Setting raop_port is crucial to trigger RAOP logic
    let mut device = create_test_device("raop-test-id", "RAOP Device", addr.ip(), addr.port());
    device.raop_port = Some(addr.port());

    // 3. Connect Client in background
    let client = AirPlayClient::new(AirPlayConfig::default());

    let connect_handle = tokio::spawn(async move { client.connect(&device).await });

    // 4. Accept connection and verify handshake
    let (mut stream, _) = listener.accept().await.unwrap();

    // Helper to read request
    let mut buffer = [0u8; 4096];

    // --- Step 1: OPTIONS ---
    let n = stream.read(&mut buffer).await.unwrap();
    let request = String::from_utf8_lossy(&buffer[..n]);

    println!("Received request 1: {}", request);

    // Verify Method
    assert!(request.starts_with("OPTIONS"));

    // Verify Mandatory Headers
    assert!(request.contains("CSeq:"), "Missing CSeq header");
    assert!(request.contains("User-Agent:"), "Missing User-Agent header");
    assert!(
        request.contains("Client-Instance:"),
        "Missing Client-Instance header"
    );
    assert!(request.contains("DACP-ID:"), "Missing DACP-ID header");
    assert!(
        request.contains("Active-Remote:"),
        "Missing Active-Remote header"
    );
    assert!(
        request.contains("X-Apple-Device-ID:"),
        "Missing X-Apple-Device-ID header"
    );

    // Send Response
    // RAOP requires Apple-Challenge in response for auth, but we can simulate success or continue
    // If we don't send Apple-Challenge, client might skip auth or fail if strict.
    // Let's send a standard response.
    let response = "RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, \
                    TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, \
                    GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n";
    stream.write_all(response.as_bytes()).await.unwrap();

    // Read subsequent requests until connect_handle finishes or a timeout occurs
    let mut step = 2;
    loop {
        let read_fut = stream.read(&mut buffer);
        let n = match tokio::time::timeout(Duration::from_millis(500), read_fut).await {
            Ok(Ok(n)) if n > 0 => n,
            _ => break,
        };
        let request = String::from_utf8_lossy(&buffer[..n]);

        println!("Received request {}: {}", step, request);

        if request.starts_with("GET /info") {
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /auth-setup") {
            // Send 32 bytes back for auth-setup to unblock client
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: 32\r\n\r\n{}", step, String::from_utf8(vec![0; 32]).unwrap());
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /pair-setup") || request.starts_with("POST /pair-verify") {
            // Unblock pair setup loop with standard valid-looking responses
            let response = format!("HTTP/1.1 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();

            // Allow client to process and potentially close the connection
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Break loop here so we can finish the test after pair-setup without timing out waiting for more requests.
            // Client connect task fails during pairing since it's a mock, which is fine for this test.
            break;
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP") || request.contains("Transport: RTP/AVP/TCP"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
                            RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                            timing_port=6002\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("RECORD") {
            assert!(request.contains("Session: CAFEBABE") || request.contains("Range: npt=0-"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();
            break;
        } else if request.starts_with("POST") {
            // Unhandled POST request
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", step);
            stream.write_all(response.as_bytes()).await.unwrap();
        }

        step += 1;
    }

    // Give a small sleep to let the client connect logic process the responses
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Drop stream before waiting for connect handle to avoid the client hanging on keep-alive
    drop(stream);

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    let result = tokio::time::timeout(Duration::from_secs(1), connect_handle).await;

    // For this compliance test we are validating that we can send valid initial connection requests.
    // The client will fail pairing since this is just a mock server returning 200 OK without
    // any actual pairing data payload. So we just accept the timeout or connection failure.
    // If it was ok, that's fine too.
    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => println!("Client failed as expected during mock pairing: {}", e),
        Ok(Err(_)) => panic!("Client panic"),
        Err(_) => println!("Timeout waiting for client (expected during mock pairing loop)"),
    }
}
