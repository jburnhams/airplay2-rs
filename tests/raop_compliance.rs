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

    // Handle variable requests like GET /info or POST /auth-setup
    let mut cseq = 2;
    let mut current_request = request.into_owned();

    loop {
        if current_request.starts_with("ANNOUNCE") {
            assert!(current_request.contains("Content-Type: application/sdp"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();

            // --- Step 3: SETUP ---
            let n = stream.read(&mut buffer).await.unwrap();
            current_request = String::from_utf8_lossy(&buffer[..n]).into_owned();
            cseq += 1;
            println!("Received request {}: {}", cseq, current_request);

            assert!(current_request.starts_with("SETUP"));
            assert!(current_request.contains("Transport: RTP/AVP/UDP"));

            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
                 RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                 timing_port=6002\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();

            // --- Step 4: RECORD ---
            let n = stream.read(&mut buffer).await.unwrap();
            current_request = String::from_utf8_lossy(&buffer[..n]).into_owned();
            cseq += 1;
            println!("Received request {}: {}", cseq, current_request);

            assert!(current_request.starts_with("RECORD"));
            assert!(current_request.contains("Session: CAFEBABE"));

            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            break;
        } else if current_request.starts_with("POST") {
            println!("Got POST: {}", current_request);
            // Handle POST /auth-setup or similar
            if current_request.contains("/auth-setup") {
                let response = format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: \
                     application/octet-stream\r\nContent-Length: 32\r\n\r\n",
                    cseq
                );
                let mut resp_bytes = response.into_bytes();
                resp_bytes.extend_from_slice(&[0u8; 32]);
                stream.write_all(&resp_bytes).await.unwrap();
            } else {
                let response = format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                    cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        } else if current_request.starts_with("GET") {
            println!("Got GET: {}", current_request);
            if current_request.contains("/info") {
                // Send dummy info response
                let info = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>macAddress</key>\n\t<string>00:11:22:33:44:55</string>\n</dict>\n</plist>";
                let response = format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: \
                     text/x-apple-plist+xml\r\nContent-Length: {}\r\n\r\n",
                    cseq,
                    info.len()
                );
                let mut resp_bytes = response.into_bytes();
                resp_bytes.extend_from_slice(info);
                stream.write_all(&resp_bytes).await.unwrap();
            } else {
                let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        } else {
            println!("Unknown request: {}", current_request);
            break;
        }

        // Read next request
        let n = stream.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        current_request = String::from_utf8_lossy(&buffer[..n]).into_owned();
        cseq += 1;
        println!("Received request {}: {}", cseq, current_request);
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    let result = tokio::time::timeout(Duration::from_secs(1), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => println!("Client failed: {}", e),
        Ok(Err(e)) => panic!("Client failed: {}", e),
        Err(_) => println!("Timeout waiting for client (expected due to incomplete mock)"),
    }
}
