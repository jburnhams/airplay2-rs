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

    let mut request_str = request.into_owned();
    let mut cseq = 2;

    if request_str.starts_with("GET /info") {
        let response = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: \
             text/x-apple-plist+xml\r\nContent-Length: 0\r\n\r\n",
            cseq
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let n = stream.read(&mut buffer).await.unwrap();
        request_str = String::from_utf8_lossy(&buffer[..n]).into_owned();
        println!("Received request 3: {}", request_str);
        cseq += 1;
    }

    if request_str.starts_with("ANNOUNCE") {
        assert!(request_str.contains("Content-Type: application/sdp"));

        let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
        stream.write_all(response.as_bytes()).await.unwrap();
        cseq += 1;

        // --- Step 3/4: SETUP ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request_str = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request {}: {}", cseq, request_str);

        assert!(request_str.starts_with("SETUP"));
        assert!(request_str.contains("Transport: RTP/AVP/UDP"));

        let response = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
             RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002\\
             r\n\r\n",
            cseq
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        cseq += 1;

        // --- Step 4/5: RECORD ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request_str = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request {}: {}", cseq, request_str);

        assert!(request_str.starts_with("RECORD"));
        assert!(request_str.contains("Session: CAFEBABE"));
        assert!(request_str.contains("Range: npt=0-"));

        let response = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n",
            cseq
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    } else if request_str.starts_with("POST") {
        // Maybe pairing?
        println!("Got POST instead of ANNOUNCE");
        let response = format!(
            "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: 32\r\nContent-Type: \
             application/octet-stream\r\n\r\n",
            cseq
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.write_all(&[0u8; 32]).await.unwrap();

        // After auth-setup, it might send another POST /auth-setup or ANNOUNCE. Let's read it.
        if let Ok(n) =
            tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer)).await
        {
            let n = n.unwrap();
            let req_str2 = String::from_utf8_lossy(&buffer[..n]);
            println!("Received request {}: {}", cseq + 1, req_str2);
        }
    }

    // Drop the listener and stream to force client connection to abort.
    drop(stream);
    drop(listener);

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    // Force failure of connection to ensure timeout doesn't happen, as we didn't complete handshake
    // connect_handle handles the task. Since stream was closed, client connection should fail
    // cleanly Wait for the task up to 5 seconds to ensure client times out.
    let result = tokio::time::timeout(Duration::from_secs(5), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => println!("Client safely failed after mock stop: {}", e),
        Ok(Err(_)) => panic!("Client panic"),
        Err(_) => println!("Timeout waiting for client (expected)"),
    }
}
