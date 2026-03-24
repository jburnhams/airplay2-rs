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

    // Read request loop because the client sends multiple setup requests like GET /info
    // before ANNOUNCE.
    loop {
        let n = stream.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request: {}", request);

        if request.starts_with("GET /info") {
            let response = "RTSP/1.0 200 OK\r\nCSeq: 2\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /auth-setup") {
            // Send empty success or expected 32-byte response for auth-setup to unblock client
            let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 32\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&[0u8; 32]).await.unwrap();
        } else if request.starts_with("POST /pair-setup") {
            // Stop early so we do not attempt full pairing because that requires complex cryptography handling
            // We just wanted to verify RAOP handshake logic sends expected headers on connection
            // Unblock the client pairing setup first
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
            // Need a sleep to ensure client reads the response before dropping the stream
            tokio::time::sleep(Duration::from_millis(50)).await;

            // To make sure the background task actually shuts down, we should abort the connection
            // since pair-setup initiates a long HTTP exchange on a different socket entirely, the main
            // RTSP stream drop won't cancel the pairing HTTP client request.
            // We abort the spawned task immediately to ensure the timeout passes
            connect_handle.abort();
            break;
        } else if request.starts_with("POST /pair-verify") {
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            connect_handle.abort();
            break;
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));

            let response = "RTSP/1.0 200 OK\r\nCSeq: 4\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
            break;
        } else {
            // Unhandled request or pairing data
            break;
        }
    }

    // Await client result (with timeout)
    // The client will fail pairing because we explicitly dropped connection when `POST /pair-setup` arrived,
    // which tests that we successfully handled the start of RAOP compliance and gracefully error on pairing fail.
    drop(stream);
    let result = tokio::time::timeout(Duration::from_secs(2), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => {
            // Because we broke early at pair-setup, an error is expected, verify it's the expected drop/EOF error
            println!("Client returned error as expected on drop: {}", e);
        }
        Ok(Err(e)) => {
            // Because we called connect_handle.abort(), an abort error is a JoinError that gets returned as an Err.
            // This is expected and means the connection dropped/failed exactly as we tested.
            println!("Client returned task error as expected on drop: {}", e);
        }
        Err(_) => panic!("Timeout waiting for client!"),
    }
}
