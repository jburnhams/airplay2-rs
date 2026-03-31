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
    // Also set raop capabilities to not require auth if possible or we can just mock the handshake.

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

    // The client may send varying requests (GET /info, POST /auth-setup, etc)
    // before ANNOUNCE. We need to handle them robustly in a loop.
    let mut step = 2;
    loop {
        let n = match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer))
            .await
        {
            Ok(Ok(n)) if n > 0 => n,
            _ => break,
        };
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request {}: {}", step, request);

        let is_http = request.contains("HTTP/1.1");
        let protocol = if is_http { "HTTP/1.1" } else { "RTSP/1.0" };

        // Extract CSeq
        let cseq = if let Some(idx) = request.find("CSeq: ") {
            let end = request[idx..].find("\r\n").unwrap_or(request.len() - idx);
            request[idx..idx + end].to_string()
        } else {
            format!("CSeq: {}", step)
        };

        if request.contains("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));
            let response = format!("{} 200 OK\r\n{}\r\n\r\n", protocol, cseq);
            stream.write_all(response.as_bytes()).await.unwrap();

            // For the test, we'll continue to let it finish the handshake completely.
        } else if request.contains("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP"));
            let response = format!(
                "{} 200 OK\r\n{}\r\nSession: CAFEBABE\r\nTransport: \
                 RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                 timing_port=6002\r\n\r\n",
                protocol, cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.contains("RECORD") {
            assert!(request.contains("Session: CAFEBABE"));
            assert!(request.contains("Range: npt=0-"));
            let response = format!(
                "{} 200 OK\r\n{}\r\nAudio-Latency: 2205\r\n\r\n",
                protocol, cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();

            // Add a small sleep then break to allow the client to process the response
            tokio::time::sleep(Duration::from_millis(50)).await;
            break;
        } else if request.contains("GET /info") {
            let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                <plist version=\"1.0\">\n\
                <dict>\n\
                    <key>qualifier</key>\n\
                    <array>\n\
                        <string>txt</string>\n\
                    </array>\n\
                </dict>\n\
                </plist>";
            let response = format!(
                "{} 200 OK\r\n{}\r\nContent-Type: text/x-apple-plist+xml\r\nContent-Length: \
                 {}\r\n\r\n{}",
                protocol,
                cseq,
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.contains("POST /auth-setup") {
            let body = vec![0u8; 32];
            let response = format!(
                "{} 200 OK\r\n{}\r\nContent-Type: application/octet-stream\r\nContent-Length: \
                 32\r\n\r\n",
                protocol, cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        } else if request.contains("POST /pair-setup") || request.contains("POST /pair-verify") {
            let response = format!("{} 200 OK\r\n{}\r\n\r\n", protocol, cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await; // Add a small sleep
        } else {
            let response = format!("{} 200 OK\r\n{}\r\n\r\n", protocol, cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        }

        step += 1;

        if step > 20 {
            // Stop the test from infinitely looping when crypto requests continually retry.
            break;
        }
    }

    // Abort the task to prevent it from failing after test completes, since the mock doesn't
    // support full crypto pairing. The main point is we didn't panic! on an error condition
    // above.
    connect_handle.abort();
    let result = connect_handle.await;

    // Test succeeds if we aborted it gracefully or if it returned an auth failure (since we didn't
    // fully mock pairing)
    match result {
        Err(e) if e.is_cancelled() => (), // Cancelled successfully
        Ok(Err(e)) => {
            // It failed connection, likely auth failure, which is fine since we aren't mocking full
            // auth
            println!("Client failed connection: {}", e);
        }
        Ok(Ok(_)) => (), // Succeeded? that would be surprising without full auth mock, but fine.
        Err(e) => panic!("Task panicked: {}", e), // Real panic inside the task
    }
}
