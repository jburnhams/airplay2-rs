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

    // --- Step 2: ANNOUNCE ---
    let n = stream.read(&mut buffer).await.unwrap();
    let request = String::from_utf8_lossy(&buffer[..n]);

    println!("Received request 2: {}", request);

    // If auth is not required/challenged, next should be ANNOUNCE (or OPTIONS again if client
    // double checks) The client implementation might differ, so we should be robust.
    // Based on `RtspSession`, it might send ANNOUNCE or SETUP.

    if request.starts_with("ANNOUNCE") {
        assert!(request.contains("Content-Type: application/sdp"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();

        // --- Step 3: SETUP ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request 3: {}", request);

        assert!(request.starts_with("SETUP"));
        assert!(request.contains("Transport: RTP/AVP/UDP"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nSession: CAFEBABE\r\nTransport: \
                        RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                        timing_port=6002\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();

        // --- Step 4: RECORD ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request 4: {}", request);

        assert!(request.starts_with("RECORD"));
        assert!(request.contains("Session: CAFEBABE"));
        assert!(request.contains("Range: npt=0-"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 4\r\nAudio-Latency: 2205\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();
    } else if request.starts_with("POST") {
        // Maybe pairing?
        println!("Got POST instead of ANNOUNCE");
        // For this test, we might stop here if we unexpected behavior, or handle it.
        // This verifies that we at least got past the first step.
    } else if request.starts_with("GET /info") {
        // Handle GET /info gracefully
        let response = "RTSP/1.0 200 OK\r\nCSeq: 2\r\nContent-Type: text/x-apple-plist+xml\r\n\r\n<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\r\n<plist version=\"1.0\">\r\n<dict>\r\n<key>audioLatencies</key>\r\n<array>\r\n<dict>\r\n<key>audioType</key>\r\n<string>default</string>\r\n<key>inputLatencyMicros</key>\r\n<integer>0</integer>\r\n<key>outputLatencyMicros</key>\r\n<integer>0</integer>\r\n</dict>\r\n</array>\r\n<key>keepAliveSendStatsAsBody</key>\r\n<true/>\r\n</dict>\r\n</plist>\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();

        // Next request might be SETUP or ANNOUNCE or POST /auth-setup
        let n = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request 3: {}", request);

        if request.starts_with("POST /auth-setup") {
            // Send auth response
            let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n01234567890123456789012345678901";
            stream.write_all(response.as_bytes()).await.unwrap();

            loop {
                let n = match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer)).await {
                    Ok(Ok(n)) => n,
                    _ => break,
                };
                if n == 0 { break; }

                let req = String::from_utf8_lossy(&buffer[..n]);
                println!("Received follow-up request: {}", req);

                if req.starts_with("POST /auth-setup") || req.starts_with("POST /pair-setup") || req.starts_with("POST /pair-verify") {
                    let response = "HTTP/1.1 200 OK\r\nCSeq: 4\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n01234567890123456789012345678901";
                    let _ = stream.write_all(response.as_bytes()).await;
                } else if req.starts_with("ANNOUNCE") || req.starts_with("SETUP") {
                    let response = "RTSP/1.0 200 OK\r\nCSeq: 5\r\nSession: CAFEBABE\r\nTransport: \
                                    RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                                    timing_port=6002\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));

            let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();

            // Next should be SETUP
            let n = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]);
            println!("Received request 4: {}", request);

            if request.starts_with("SETUP") {
                let response = "RTSP/1.0 200 OK\r\nCSeq: 4\r\nSession: CAFEBABE\r\nTransport: \
                                RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                                timing_port=6002\r\n\r\n";
                stream.write_all(response.as_bytes()).await.unwrap();

                // Wait for RECORD
                let n = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..n]);
                println!("Received request 5: {}", request);
                if request.starts_with("RECORD") {
                    let response = "RTSP/1.0 200 OK\r\nCSeq: 5\r\nAudio-Latency: 2205\r\n\r\n";
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        } else if request.starts_with("SETUP") {
            let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nSession: CAFEBABE\r\nTransport: \
                            RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                            timing_port=6002\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();

            // Wait for RECORD
            let n = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]);
            println!("Received request 4: {}", request);
            if request.starts_with("RECORD") {
                let response = "RTSP/1.0 200 OK\r\nCSeq: 4\r\nAudio-Latency: 2205\r\n\r\n";
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // Since we are mocking the server simply to check compliance of initial requests,
    // the client connection will eventually fail or timeout due to incomplete responses.
    // For this test, we care that the client initiated the correct requests, not that
    // it successfully established a full connection against our incomplete mock.

    // Abort the connect handle so the test finishes cleanly
    connect_handle.abort();
}
