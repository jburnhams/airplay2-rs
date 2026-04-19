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

    // Handle variable subsequent requests like GET /info, POST /auth-setup, or ANNOUNCE
    // Mock servers in tests should use a robust read loop to anticipate and handle these variable request sequences
    loop {
        let n = stream.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request: {}", request);

        let mut cseq = "2";
        for line in request.lines() {
            if let Some(rest) = line.strip_prefix("CSeq: ") {
                cseq = rest.trim();
                break;
            }
        }

        if request.starts_with("GET /info") {
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/x-apple-binary-plist\r\nContent-Length: 0\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /auth-setup") {
            // Send 32-byte binary response
            let response_headers = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n", cseq);
            stream.write_all(response_headers.as_bytes()).await.unwrap();
            stream.write_all(&[0u8; 32]).await.unwrap();

            // Add a small sleep before dropping the socket to ensure the client actually receives the response
            // and doesn't encounter a connection reset.
            tokio::time::sleep(Duration::from_millis(50)).await;
            break; // Stop here for basic compliance test
        } else if request.starts_with("POST /pair-setup") {
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            break;
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
                            RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                            timing_port=6002\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("RECORD") {
            assert!(request.contains("Session: CAFEBABE"));
            assert!(request.contains("Range: npt=0-"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            break; // Finished handshake
        }
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early (e.g. after auth-setup), but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    let result = tokio::time::timeout(Duration::from_secs(12), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => {
            // If we aborted early during pairing/auth, an AuthenticationFailed error is expected and acceptable.
            let err_str = e.to_string();
            if !err_str.contains("AuthenticationFailed") && !err_str.contains("timeout") {
                panic!("Client failed unexpectedly: {}", e);
            }
        }
        Ok(Err(e)) => std::panic::resume_unwind(e.into_panic()),
        Err(_) => panic!("Timeout waiting for client"),
    }
}
