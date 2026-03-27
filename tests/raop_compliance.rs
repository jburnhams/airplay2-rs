use std::time::Duration;

use airplay2::testing::create_test_device;
use airplay2::{UnifiedAirPlayClient, ClientConfig, PreferredProtocol};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_raop_handshake_compliance() {
    // 1. Setup Custom Mock Server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 2. Create RAOP Device
    let mut device = create_test_device("raop-test-id", "RAOP Device", addr.ip(), addr.port());
    device.raop_port = Some(addr.port());
    device.capabilities.airplay2 = false; // Force fallback to RAOP

    // 3. Connect Client in background
    let mut config = ClientConfig::default();
    config.preferred_protocol = PreferredProtocol::PreferRaop;
    let mut client = UnifiedAirPlayClient::with_config(config);

    let connect_handle = tokio::spawn(async move { client.connect(device).await });

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
    // X-Apple-Device-ID is not always present in RAOP (often uses Client-Instance instead or iTunes Agent)

    // Send Response
    // RAOP requires Apple-Challenge in response for auth, but we can simulate success or continue
    // If we don't send Apple-Challenge, client might skip auth or fail if strict.
    // Let's send a standard response.
    let response = "RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, \
                    TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, \
                    GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n";
    stream.write_all(response.as_bytes()).await.unwrap();

    let mut step = 2;
    loop {
        let n = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request {}: {}", step, request);
        step += 1;

        // Find CSeq to reply properly
        let mut cseq = "1";
        for line in request.lines() {
            if line.starts_with("CSeq: ") {
                cseq = line.trim_start_matches("CSeq: ").trim();
            }
        }

        if request.starts_with("GET /info") {
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/x-apple-binary-plist\r\nContent-Length: 0\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /auth-setup") {
            // Auth-setup expects 32-byte binary response
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n",
                cseq
            );
            let mut buf = response.into_bytes();
            buf.extend_from_slice(&[0u8; 32]);
            stream.write_all(&buf).await.unwrap();
        } else if request.starts_with("POST /pair-setup") {
            let response = format!(
                "HTTP/1.1 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /fp-setup") {
            // AirPlay 2 may try FairPlay setup (fp-setup).
            // A 200 OK with no body might make it proceed or skip to pairing.
            // Returning an error (e.g. 501 Not Implemented or 404 Not Found) will force it to try other methods.
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST ") && request.contains("auth-setup") {
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n",
                cseq
            );
            let mut buf = response.into_bytes();
            buf.extend_from_slice(&[0u8; 32]);
            stream.write_all(&buf).await.unwrap();
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP"));
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
                RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                timing_port=6002\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("RECORD") {
            assert!(request.contains("Session: CAFEBABE"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            break;
        } else if request.starts_with("OPTIONS") {
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    let result = tokio::time::timeout(Duration::from_secs(1), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => panic!("Client failed: {}", e),
        Ok(Err(_)) => panic!("Client panic"),
        Err(_) => panic!("Timeout waiting for client"),
    }
}
